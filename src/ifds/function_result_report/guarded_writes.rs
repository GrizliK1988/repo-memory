//! Attribute an added loop guard through the write it selects at a return.

use super::*;
use crate::ifds::loops::{LoopOperation, LoopSummary};
use crate::ifds::model::{NodeId, Source};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardedWriteCause {
    /// All after-side evidence starts at this guard's observation.
    pub guard: ControlCauseSide,
    /// Paths start at the before/after observation's return dependency root.
    pub before_observation_index: usize,
    pub before_write: DependencyEvidence,
    pub after_write: DependencyEvidence,
    pub write_text: String,
    /// Includes the false arm, whose unchanged source has also been checked.
    pub regions: Vec<usize>,
    pub affected_regions: Vec<usize>,
}

fn unique_guards(side: &FunctionSnapshotResult) -> Option<BTreeMap<String, &ResultGuard>> {
    let mut branches = BTreeMap::<NodeId, (String, &ResultGuard)>::new();
    for guard in side.observations.iter().flat_map(|item| &item.guards) {
        let key = dependency_key(&guard.condition, side)?;
        if let Some((previous, _)) = branches.get(&guard.branch) {
            if previous != &key {
                return None;
            }
        } else {
            branches.insert(guard.branch.clone(), (key, guard));
        }
    }
    let mut result = BTreeMap::new();
    for (key, guard) in branches.into_values() {
        if result.insert(key, guard).is_some() {
            return None;
        }
    }
    Some(result)
}

fn source_operations(summary: &LoopSummary) -> Vec<&LoopOperation> {
    summary
        .operations
        .iter()
        .filter(|operation| {
            matches!(
                operation.operation.as_str(),
                "write" | "condition" | "return" | "break" | "continue" | "unknown"
            )
        })
        .collect()
}

struct LoopPair<'a> {
    before: &'a LoopSummary,
    after: &'a LoopSummary,
    write: &'a LoopOperation,
}

fn loop_pair<'a>(
    before: &'a FunctionSnapshotResult,
    after: &'a FunctionSnapshotResult,
    guard: &ResultGuard,
) -> Option<LoopPair<'a>> {
    // Use the innermost containing loop. Ancestors also record nested operations.
    let right = after
        .loops
        .iter()
        .filter(|summary| {
            summary
                .operations
                .iter()
                .any(|operation| operation.node == guard.branch)
        })
        .min_by_key(|summary| summary.span.byte_end - summary.span.byte_start)?;
    let mut candidates = before.loops.iter().filter(|summary| {
        summary.kind == right.kind
            && summary.condition_text == right.condition_text
            && summary.initializer == right.initializer
            && summary.update == right.update
    });
    let left = candidates.next()?;
    if candidates.next().is_some()
        || after
            .loops
            .iter()
            .filter(|summary| {
                summary.kind == right.kind && summary.condition_text == right.condition_text
            })
            .count()
            != 1
        || left.feasibility != Coverage::Complete
        || right.feasibility != Coverage::Complete
        || left.may_enter != right.may_enter
        || left.may_skip != right.may_skip
        || left.may_repeat
        || right.may_repeat
    {
        return None;
    }
    let old = source_operations(left);
    let new = source_operations(right);
    let mut added_writes = new.iter().filter(|operation| {
        operation.operation == "write"
            && !old.iter().any(|previous| {
                previous.operation == operation.operation && previous.text == operation.text
            })
    });
    let write = *added_writes.next()?;
    if added_writes.next().is_some() || write.may_repeat {
        return None;
    }
    let remaining = new
        .iter()
        .filter(|operation| operation.node != guard.branch && operation.node != write.node)
        .map(|operation| (&operation.operation, &operation.text));
    if !old
        .iter()
        .map(|operation| (&operation.operation, &operation.text))
        .eq(remaining)
    {
        return None;
    }
    Some(LoopPair {
        before: left,
        after: right,
        write,
    })
}

// Source positions before/after the edited loop retain their relative offsets.
// Inside it, match unique source operations and offsets within their snippets.
fn source_position(
    span: &SourceSpan,
    side: &FunctionSnapshotResult,
    summary: &LoopSummary,
) -> Option<String> {
    if span.path != side.function.declaration.path {
        return None;
    }
    let length = span.byte_end.checked_sub(span.byte_start)?;
    if span.byte_end <= summary.span.byte_start {
        return Some(format!(
            "before:{}:{length}",
            span.byte_start
                .checked_sub(side.function.declaration.byte_start)?
        ));
    }
    if span.byte_start >= summary.span.byte_end {
        return Some(format!(
            "after:{}:{length}",
            span.byte_start - summary.span.byte_end
        ));
    }
    let operation = summary
        .operations
        .iter()
        .filter(|operation| {
            operation.span.byte_start <= span.byte_start && operation.span.byte_end >= span.byte_end
        })
        .min_by_key(|operation| operation.span.byte_end - operation.span.byte_start)?;
    if summary
        .operations
        .iter()
        .filter(|item| item.operation == operation.operation && item.text == operation.text)
        .count()
        != 1
    {
        return None;
    }
    Some(format!(
        "inside:{}:{}:{}:{length}",
        operation.operation,
        operation.text,
        span.byte_start - operation.span.byte_start
    ))
}

fn same_span(
    a: Option<&SourceSpan>,
    b: Option<&SourceSpan>,
    left: &FunctionSnapshotResult,
    right: &FunctionSnapshotResult,
    pair: &LoopPair<'_>,
) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => source_position(a, left, pair.before)
            .is_some_and(|key| source_position(b, right, pair.after).as_ref() == Some(&key)),
        _ => false,
    }
}

fn evidence(dependency: &ResultDependency, path: &[usize]) -> DependencyEvidence {
    DependencyEvidence {
        path: path.to_vec(),
        node: dependency.node.clone(),
        operation: dependency.operation.clone(),
        span: dependency.span.clone(),
    }
}

fn same_dependency(
    a: &ResultDependency,
    b: &ResultDependency,
    sides: (&FunctionSnapshotResult, &FunctionSnapshotResult),
    pair: &LoopPair<'_>,
    replace: bool,
    path: &mut Vec<usize>,
    replacements: &mut Vec<(DependencyEvidence, DependencyEvidence)>,
) -> Option<()> {
    let (left, right) = sides;
    if a.unresolved || b.unresolved || a.role != b.role {
        return None;
    }
    if replace && b.node == pair.write.node {
        let old_write = pair
            .before
            .operations
            .iter()
            .find(|operation| operation.node == a.node && operation.operation == "write")?;
        let target = |text: &str| {
            text.split_once('=')
                .map(|(target, _)| target.trim().to_owned())
        };
        if a.operation != "write"
            || b.operation != "write"
            || target(&old_write.text).is_none()
            || target(&old_write.text) != target(&pair.write.text)
        {
            return None;
        }
        replacements.push((evidence(a, path), evidence(b, path)));
        return Some(());
    }
    if a.operation != b.operation
        || a.literal != b.literal
        || a.operator != b.operator
        || a.inputs.len() != b.inputs.len()
        || !same_span(a.span.as_ref(), b.span.as_ref(), left, right, pair)
    {
        return None;
    }
    match (&a.origin, &b.origin) {
        (None, None) | (Some(Source::Write(_)), Some(Source::Write(_))) => {}
        (Some(Source::FunctionInput(a)), Some(Source::FunctionInput(b)))
            if left.parameters.iter().position(|item| item == a)
                == right.parameters.iter().position(|item| item == b) => {}
        _ => return None,
    }
    for (index, (a, b)) in a.inputs.iter().zip(&b.inputs).enumerate() {
        path.push(index);
        same_dependency(a, b, sides, pair, replace, path, replacements)?;
        path.pop();
    }
    Some(())
}

pub(super) fn guarded_write_causes(
    analysis: &FunctionResultAnalysis,
    comparison: &FunctionResultComparison,
) -> Vec<GuardedWriteCause> {
    guarded_write_cause(analysis, comparison)
        .into_iter()
        .collect()
}

fn guarded_write_cause(
    analysis: &FunctionResultAnalysis,
    comparison: &FunctionResultComparison,
) -> Option<GuardedWriteCause> {
    if analysis.counterpart_status != FunctionCounterpart::Matched
        || analysis.input_mapping_coverage != Coverage::Complete
        || comparison.comparison_coverage != Coverage::Complete
        || comparison.before_entry != comparison.after_entry
        || comparison.common_domain_status != CommonDomainStatus::Present
        || !comparison.unresolved_domain.is_empty()
        || !comparison.limits_hit.is_empty()
    {
        return None;
    }
    let left = analysis.before.as_ref()?;
    let right = analysis.after.as_ref()?;
    if left.coverage != Coverage::Complete
        || right.coverage != Coverage::Complete
        || !left.limits_hit.is_empty()
        || !right.limits_hit.is_empty()
        || left.parameters.len() != right.parameters.len()
        || !left
            .parameters
            .iter()
            .zip(&right.parameters)
            .enumerate()
            .all(|(slot, (a, b))| {
                analysis.input_correspondence.iter().any(|input| {
                    input.position == slot as u32 && &input.before == a && &input.after == b
                })
            })
    {
        return None;
    }
    // A unique return site avoids inventing correspondence between return edits.
    for side in [left, right] {
        if side
            .observations
            .iter()
            .map(|item| &item.site)
            .collect::<BTreeSet<_>>()
            .len()
            != 1
        {
            return None;
        }
    }
    let old_guards = unique_guards(left)?;
    let new_guards = unique_guards(right)?;
    if old_guards.keys().any(|key| !new_guards.contains_key(key)) {
        return None;
    }
    let mut added = new_guards
        .iter()
        .filter(|(key, _)| !old_guards.contains_key(*key));
    let (_, added_guard) = added.next()?;
    if added.next().is_some() {
        return None;
    }
    let pair = loop_pair(left, right, added_guard)?;
    let mut cause: Option<GuardedWriteCause> = None;
    let mut regions = Vec::new();
    let mut affected_regions = Vec::new();
    for (region_index, region) in comparison.regions.iter().enumerate() {
        let ([a_ref], [b_ref]) = (
            region.before_observations.as_slice(),
            region.after_observations.as_slice(),
        ) else {
            return None;
        };
        let (a_index, a) = observation(&analysis.before, a_ref)?;
        let (b_index, b) = observation(&analysis.after, b_ref)?;
        if a.dependency_coverage != Coverage::Complete
            || b.dependency_coverage != Coverage::Complete
            || a.unknown_completion_before_return
            || b.unknown_completion_before_return
            || a.kind != b.kind
            || !same_span(Some(&a.span), Some(&b.span), left, right, &pair)
        {
            return None;
        }
        let retained: Vec<_> = b
            .guards
            .iter()
            .filter(|guard| guard.branch != added_guard.branch)
            .collect();
        if a.guards.len() != retained.len() {
            return None;
        }
        for (old, new) in a.guards.iter().zip(retained) {
            if old.outcome != new.outcome {
                return None;
            }
            same_dependency(
                &old.condition,
                &new.condition,
                (left, right),
                &pair,
                false,
                &mut Vec::new(),
                &mut Vec::new(),
            )?;
        }
        let (
            ResultValue::Expression {
                text: a_text,
                dependency: a_dep,
                ..
            },
            ResultValue::Expression {
                text: b_text,
                dependency: b_dep,
                ..
            },
        ) = (&a.value, &b.value)
        else {
            return None;
        };
        if a_text != b_text {
            return None;
        }
        let new_guard = b
            .guards
            .iter()
            .enumerate()
            .find(|(_, guard)| guard.branch == added_guard.branch);
        let active = new_guard.is_some_and(|(_, guard)| guard.outcome);
        let mut replacements = Vec::new();
        same_dependency(
            a_dep,
            b_dep,
            (left, right),
            &pair,
            active,
            &mut Vec::new(),
            &mut replacements,
        )?;
        if let Some((guard_index, guard)) = new_guard {
            regions.push(region_index);
            if active {
                let [(before_write, after_write)] = replacements.as_slice() else {
                    return None;
                };
                if region.assessment != ResultAssessment::Different {
                    return None;
                }
                affected_regions.push(region_index);
                let next = GuardedWriteCause {
                    guard: ControlCauseSide {
                        observation_index: b_index,
                        guard_index,
                        branch: guard.branch.clone(),
                        condition: dependency_expression(
                            &guard.condition,
                            right,
                            &left.parameter_names,
                        )?
                        .0,
                        edit: evidence(&guard.condition, &[]),
                        expression: dependency_expression(
                            &guard.condition,
                            right,
                            &left.parameter_names,
                        )?
                        .0,
                    },
                    before_observation_index: a_index,
                    before_write: before_write.clone(),
                    after_write: after_write.clone(),
                    write_text: pair.write.text.clone(),
                    regions: Vec::new(),
                    affected_regions: Vec::new(),
                };
                if let Some(previous) = &cause {
                    if previous.before_write.node != next.before_write.node
                        || previous.after_write.node != next.after_write.node
                    {
                        return None;
                    }
                } else {
                    cause = Some(next);
                }
            } else if region.assessment != ResultAssessment::Equal {
                return None;
            }
        } else if region.assessment != ResultAssessment::Equal {
            return None;
        }
    }
    let mut cause = cause?;
    // Require represented evidence for the guard's unchanged alternative too.
    if regions.len() == affected_regions.len() {
        return None;
    }
    cause.regions = regions;
    cause.affected_regions = affected_regions;
    Some(cause)
}

pub(super) fn link_guarded_write_causes(
    findings: &mut [FunctionResultFinding],
    causes: &[GuardedWriteCause],
) {
    for finding in findings {
        for (index, cause) in causes.iter().enumerate() {
            if !finding.regions.is_empty()
                && finding
                    .regions
                    .iter()
                    .all(|region| cause.regions.contains(region))
            {
                finding.guarded_write_cause_refs.push(index);
                finding.attribution_certain = true;
            }
        }
    }
}

pub(super) fn guarded_write_cause_lines(
    compact: &FunctionResultCompactReport,
    full: &FunctionResultReport,
) -> Vec<String> {
    compact.findings.iter().flat_map(|finding| &finding.cause_refs)
        .filter(|reference| reference.section == "guarded_write_causes")
        .map(|reference| reference.index).collect::<BTreeSet<_>>().into_iter().map(|index| {
            let cause = &full.guarded_write_causes[index];
            let guard = cause.guard.edit.span.as_ref().expect("guard cause has span");
            let write = cause.after_write.span.as_ref().expect("write cause has span");
            let item = &full.analysis.after.as_ref().unwrap().observations[cause.guard.observation_index];
            let ResultValue::Expression { text, .. } = &item.value else { unreachable!() };
            format!("Cause: added condition {} (after {}:{}) selects {} (after {}:{}), which reaches return {} (after {}:{})",
                cause.guard.condition, guard.path, guard.start_line, cause.write_text,
                write.path, write.start_line, text, item.span.path, item.span.start_line)
        }).collect()
}
