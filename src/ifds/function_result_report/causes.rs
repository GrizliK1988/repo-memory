//! Source attribution for a changed guard selecting otherwise unchanged returns.

use super::*;
use crate::ifds::model::{NodeId, Source};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionResultCauseKind {
    ControlSourceChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlCauseSide {
    /// Observation and guard indices locate the dependency root in full evidence.
    pub observation_index: usize,
    pub guard_index: usize,
    pub branch: NodeId,
    pub condition: String,
    /// The dependency path starts at this guard's condition, not its return value.
    pub edit: DependencyEvidence,
    /// Expanded input expression at the source edit, without runtime evaluation.
    pub expression: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionResultCause {
    pub kind: FunctionResultCauseKind,
    pub before: ControlCauseSide,
    pub after: ControlCauseSide,
    /// Absolute indices into comparison.regions. A control effect may preserve
    /// the returned computation; it is still explained by this guard edit.
    pub regions: Vec<usize>,
}

type Anchor = (u32, u32);
type WriteSources<'a> = BTreeMap<Anchor, (&'a ResultDependency, Vec<usize>, String)>;

fn anchor(span: &SourceSpan, side: &FunctionSnapshotResult) -> Option<Anchor> {
    (span.path == side.function.declaration.path).then_some(())?;
    Some((
        span.start_line
            .checked_sub(side.function.declaration.start_line)?,
        span.end_line
            .checked_sub(side.function.declaration.start_line)?,
    ))
}

/// Describe this operation's syntax without expanding previous assignments.
/// Reads are anchored to the precise reaching definition, so a copied upstream
/// change is not mistaken for a new edit at every downstream copy.
fn local_operation(dependency: &ResultDependency, side: &FunctionSnapshotResult) -> Option<String> {
    if dependency.unresolved {
        return None;
    }
    if let Some(Source::FunctionInput(binding)) = &dependency.origin {
        return Some(format!(
            "input:{}",
            side.parameters.iter().position(|p| p == binding)?
        ));
    }
    if dependency.operation == "write" {
        return Some(format!(
            "write:{:?}",
            anchor(dependency.span.as_ref()?, side)?
        ));
    }
    if dependency.operation == "binding_value" {
        let [input] = dependency.inputs.as_slice() else {
            return None;
        };
        return local_operation(input, side);
    }
    if matches!(
        dependency.operator,
        Some(crate::ifds::ir::PrimitiveOperator::ValueJoin { .. })
    ) {
        let [input] = dependency.inputs.as_slice() else {
            return None;
        };
        return local_operation(input, side);
    }
    let children = dependency
        .inputs
        .iter()
        .map(|input| Some((input.role.clone(), local_operation(input, side)?)))
        .collect::<Option<Vec<_>>>()?;
    Some(
        serde_json::to_string(&(
            &dependency.operation,
            &dependency.operator,
            &dependency.literal,
            children,
        ))
        .expect("local operation serializes"),
    )
}

fn writes<'a>(
    root: &'a ResultDependency,
    side: &FunctionSnapshotResult,
) -> Option<WriteSources<'a>> {
    fn visit<'a>(
        dependency: &'a ResultDependency,
        side: &FunctionSnapshotResult,
        path: &mut Vec<usize>,
        output: &mut WriteSources<'a>,
    ) -> Option<()> {
        if dependency.unresolved {
            return None;
        }
        if dependency.operation == "write" {
            let key = anchor(dependency.span.as_ref()?, side)?;
            let expression = dependency
                .inputs
                .iter()
                .map(|input| local_operation(input, side))
                .collect::<Option<Vec<_>>>()?;
            let expression = serde_json::to_string(&expression).expect("write sources serialize");
            if let Some((previous, _, old)) = output.get(&key) {
                if previous.node != dependency.node || old != &expression {
                    return None;
                }
            } else {
                output.insert(key, (dependency, path.clone(), expression));
            }
        }
        for (index, input) in dependency.inputs.iter().enumerate() {
            path.push(index);
            visit(input, side, path, output)?;
            path.pop();
        }
        Some(())
    }
    let mut output = BTreeMap::new();
    visit(root, side, &mut Vec::new(), &mut output)?;
    Some(output)
}

struct GuardSource<'a> {
    observation_index: usize,
    guard_index: usize,
    guard: &'a ResultGuard,
    key: String,
    signature: String,
}

fn guard_sources(side: &FunctionSnapshotResult) -> Option<BTreeMap<Anchor, GuardSource<'_>>> {
    let mut output: BTreeMap<Anchor, GuardSource<'_>> = BTreeMap::new();
    for (observation_index, item) in side.observations.iter().enumerate() {
        for (guard_index, guard) in item.guards.iter().enumerate() {
            let key = dependency_key(&guard.condition, side)?;
            let sources = writes(&guard.condition, side)?;
            let signature = serde_json::to_string(&(
                local_operation(&guard.condition, side)?,
                sources
                    .iter()
                    .map(|(anchor, (_, _, expression))| (anchor, expression))
                    .collect::<Vec<_>>(),
            ))
            .expect("guard sources serialize");
            let location = anchor(guard.span.as_ref()?, side)?;
            if let Some(previous) = output.get(&location) {
                if previous.guard.branch != guard.branch
                    || previous.key != key
                    || previous.signature != signature
                {
                    return None;
                }
            } else {
                output.insert(
                    location,
                    GuardSource {
                        observation_index,
                        guard_index,
                        guard,
                        key,
                        signature,
                    },
                );
            }
        }
    }
    Some(output)
}

fn cause_side(
    source: &GuardSource<'_>,
    dependency: &ResultDependency,
    path: Vec<usize>,
    side: &FunctionSnapshotResult,
    names: &[String],
) -> Option<ControlCauseSide> {
    Some(ControlCauseSide {
        observation_index: source.observation_index,
        guard_index: source.guard_index,
        branch: source.guard.branch.clone(),
        condition: dependency_expression(&source.guard.condition, side, names)?.0,
        edit: DependencyEvidence {
            path,
            node: dependency.node.clone(),
            operation: dependency.operation.clone(),
            span: Some(dependency.span.clone()?),
        },
        expression: dependency_expression(dependency, side, names)?.0,
    })
}

pub(super) fn control_change_causes(
    analysis: &FunctionResultAnalysis,
    comparison: &FunctionResultComparison,
) -> Vec<FunctionResultCause> {
    control_change_cause(analysis, comparison)
        .into_iter()
        .collect()
}

fn control_change_cause(
    analysis: &FunctionResultAnalysis,
    comparison: &FunctionResultComparison,
) -> Option<FunctionResultCause> {
    if analysis.counterpart_status != FunctionCounterpart::Matched
        || analysis.input_mapping_coverage != Coverage::Complete
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
    {
        return None;
    }
    for (slot, (a, b)) in left.parameters.iter().zip(&right.parameters).enumerate() {
        if !analysis
            .input_correspondence
            .iter()
            .any(|pair| pair.position == slot as u32 && pair.before == *a && pair.after == *b)
        {
            return None;
        }
    }
    let before = guard_sources(left)?;
    let after = guard_sources(right)?;
    if before.keys().ne(after.keys()) {
        return None;
    }
    let before_guards = before
        .values()
        .map(|source| source.guard)
        .collect::<Vec<_>>();
    let after_guards = after
        .values()
        .map(|source| source.guard)
        .collect::<Vec<_>>();
    if observation_shapes(left, &before_guards)? != observation_shapes(right, &after_guards)? {
        return None;
    }
    let changed = before
        .iter()
        .filter_map(|(key, a)| {
            let b = &after[key];
            (a.key != b.key).then_some((a, b))
        })
        .collect::<Vec<_>>();
    let [(a, b)] = changed.as_slice() else {
        return None;
    };
    let old_writes = writes(&a.guard.condition, left)?;
    let new_writes = writes(&b.guard.condition, right)?;
    let edits = old_writes
        .iter()
        .filter_map(|(key, (old, path, syntax))| {
            let (new, new_path, new_syntax) = new_writes.get(key)?;
            (syntax != new_syntax).then_some((*old, path.clone(), *new, new_path.clone()))
        })
        .collect::<Vec<_>>();
    let (old, old_path, new, new_path) = match edits.as_slice() {
        [(old, path, new, new_path)]
            if local_operation(&a.guard.condition, left)?
                == local_operation(&b.guard.condition, right)? =>
        {
            (*old, path.clone(), *new, new_path.clone())
        }
        [] if old_writes.keys().eq(new_writes.keys())
            && local_operation(&a.guard.condition, left)?
                != local_operation(&b.guard.condition, right)? =>
        {
            // Direct predicate edits have no changed assignment to trace to.
            (
                &a.guard.condition,
                Vec::new(),
                &b.guard.condition,
                Vec::new(),
            )
        }
        _ => return None,
    };
    // Changes at other common upstream definitions would make this attribution
    // ambiguous, even if their effect happens to be hidden by the selected edit.
    let names = &left.parameter_names;
    let before = cause_side(a, old, old_path, left, names)?;
    let after = cause_side(b, new, new_path, right, names)?;
    let mut regions = Vec::new();
    let mut changed_return = false;
    for (index, region) in comparison.regions.iter().enumerate() {
        if !region.control_changed {
            continue;
        }
        let paired = |side: &Option<FunctionSnapshotResult>,
                      refs: &[ResultObservationRef],
                      branch: &NodeId| {
            !refs.is_empty()
                && refs.iter().all(|reference| {
                    observation(side, reference).is_some_and(|(_, item)| {
                        item.dependency_coverage == Coverage::Complete
                            && !item.unknown_completion_before_return
                            && item.guards.iter().any(|guard| &guard.branch == branch)
                    })
                })
        };
        if !paired(
            &analysis.before,
            &region.before_observations,
            &a.guard.branch,
        ) || !paired(&analysis.after, &region.after_observations, &b.guard.branch)
        {
            continue;
        }
        if region.value_dependency_changed {
            if matches!(
                region.assessment,
                ResultAssessment::Equal | ResultAssessment::Unknown
            ) {
                return None;
            }
            changed_return = true;
        }
        regions.push(index);
    }
    changed_return.then_some(FunctionResultCause {
        kind: FunctionResultCauseKind::ControlSourceChanged,
        before,
        after,
        regions,
    })
}

pub(super) fn link_control_causes(
    findings: &mut [FunctionResultFinding],
    causes: &[FunctionResultCause],
) {
    for finding in findings {
        for (index, cause) in causes.iter().enumerate() {
            if !finding.regions.is_empty()
                && finding
                    .regions
                    .iter()
                    .all(|region| cause.regions.contains(region))
            {
                finding.cause_refs.push(index);
                finding.attribution_certain = true;
            }
        }
    }
}

pub(super) fn control_cause_lines(
    compact: &FunctionResultCompactReport,
    full: &FunctionResultReport,
) -> Vec<String> {
    let visible: BTreeSet<_> = compact
        .findings
        .iter()
        .flat_map(|finding| &finding.cause_refs)
        .map(|reference| reference.index)
        .collect();
    visible
        .into_iter()
        .flat_map(|index| {
            let cause = &full.causes[index];
            let a = cause
                .before
                .edit
                .span
                .as_ref()
                .expect("source cause has span");
            let b = cause
                .after
                .edit
                .span
                .as_ref()
                .expect("source cause has span");
            vec![format!(
                "Edit: {} -> {} (before {}:{}; after {}:{})",
                cause.before.expression,
                cause.after.expression,
                a.path,
                a.start_line,
                b.path,
                b.start_line
            )]
        })
        .collect()
}
