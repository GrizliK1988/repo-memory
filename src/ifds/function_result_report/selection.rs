//! Explain a changed pure guard through the return source it selects.

use super::*;
use crate::ifds::model::NodeId;

pub(super) struct ReturnSelectionRule {
    pub pair: GuardPair,
    pub lines: Vec<String>,
}

type InputGuard = (u32, bool);

fn relative_lines(span: &SourceSpan, side: &FunctionSnapshotResult) -> Option<(u32, u32)> {
    (span.path == side.function.declaration.path).then_some(())?;
    Some((
        span.start_line
            .checked_sub(side.function.declaration.start_line)?,
        span.end_line
            .checked_sub(side.function.declaration.start_line)?,
    ))
}

fn guards(side: &FunctionSnapshotResult) -> Option<Vec<&ResultGuard>> {
    let mut branches: BTreeMap<&NodeId, &ResultGuard> = BTreeMap::new();
    for guard in side.observations.iter().flat_map(|item| &item.guards) {
        let key = input_guard(&guard.condition, side)?;
        if let Some(previous) = branches.insert(&guard.branch, guard)
            && (input_guard(&previous.condition, side)? != key || previous.span != guard.span)
        {
            return None;
        }
    }
    let mut guards: Vec<_> = branches.into_values().collect();
    guards.sort_by_key(|guard| guard.span.as_ref().map(|span| span.byte_start));
    // Multiple branches on one line have no unambiguous relative-line anchor.
    let lines: BTreeSet<_> = guards
        .iter()
        .map(|guard| relative_lines(guard.span.as_ref()?, side))
        .collect::<Option<_>>()?;
    (lines.len() == guards.len()).then_some(guards)
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ReturnSource {
    kind: String,
    site: (u32, u32),
    value: String,
    writes: Vec<(u32, u32)>,
}

pub(super) type ObservationShapes = BTreeMap<(ReturnSource, Vec<(usize, bool)>), usize>;

fn return_source(item: &ResultObservation, side: &FunctionSnapshotResult) -> Option<ReturnSource> {
    if item.dependency_coverage != Coverage::Complete || item.unknown_completion_before_return {
        return None;
    }
    let (value, writes) = match &item.value {
        ResultValue::Undefined => ("undefined".into(), Vec::new()),
        ResultValue::Expression { dependency, .. } => (
            dependency_key(dependency, side)?,
            key_write_spans(dependency)
                .values()
                .map(|span| relative_lines(span, side))
                .collect::<Option<Vec<_>>>()?,
        ),
    };
    // Snapshot node IDs do not define correspondence. Keep physical write
    // anchors, operation/operand identity and return sites independently.
    let writes = writes
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Some(ReturnSource {
        kind: format!("{:?}", item.kind),
        site: relative_lines(&item.span, side)?,
        value,
        writes,
    })
}

pub(super) fn observation_shapes(
    side: &FunctionSnapshotResult,
    branches: &[&ResultGuard],
) -> Option<ObservationShapes> {
    let mut shapes = BTreeMap::new();
    for item in &side.observations {
        let guards = item
            .guards
            .iter()
            .map(|guard| {
                Some((
                    branches
                        .iter()
                        .position(|branch| branch.branch == guard.branch)?,
                    guard.outcome,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        *shapes
            .entry((return_source(item, side)?, guards))
            .or_default() += 1;
    }
    Some(shapes)
}

fn selected_source(
    side: &Option<FunctionSnapshotResult>,
    flows: &[CompactSideFlow],
    branch: &NodeId,
    names: &[String],
) -> Option<(ReturnSource, String, Vec<BooleanRegion>)> {
    let snapshot = side.as_ref()?;
    let mut selected = BTreeMap::new();
    let mut fallback = BTreeSet::new();
    for flow in flows {
        if flow.flow.completion != Coverage::Complete {
            return None;
        }
        let [reference] = flow.flow.observations.as_slice() else {
            return None;
        };
        let (_, item) = observation(side, reference)?;
        let Some(guard) = item.guards.iter().find(|guard| &guard.branch == branch) else {
            continue;
        };
        let source = return_source(item, snapshot)?;
        if guard.outcome {
            let ResultValue::Expression { dependency, .. } = &item.value else {
                return None;
            };
            let (expression, _) = dependency_expression(dependency, snapshot, names)?;
            let (label, regions) = selected
                .entry(source)
                .or_insert_with(|| (expression.clone(), Vec::new()));
            if *label != expression {
                return None;
            }
            regions.push(flow.flow.region.clone());
        } else {
            fallback.insert(source.value);
        }
    }
    if selected.len() != 1 {
        return None;
    }
    let (source, (expression, regions)) = selected.pop_first()?;
    // An identical returned computation on the false arm does not identify a
    // distinct selected source, even if its return location differs.
    if fallback.is_empty() || fallback.contains(&source.value) {
        return None;
    }
    Some((source, expression, simplify_regions(regions)))
}

pub(super) fn changed_return_selection(
    compact: &FunctionResultCompactReport,
    full: &FunctionResultReport,
) -> Option<ReturnSelectionRule> {
    if compact.coverage.presentation != Coverage::Complete
        || full.analysis.counterpart_status != FunctionCounterpart::Matched
        || full.comparison.input_mapping_coverage != Coverage::Complete
        || full.comparison.before_entry != full.comparison.after_entry
        || full.comparison.common_domain_status != CommonDomainStatus::Present
        || !full.comparison.unresolved_domain.is_empty()
        || !full.comparison.limits_hit.is_empty()
    {
        return None;
    }
    let left = full.analysis.before.as_ref()?;
    let right = full.analysis.after.as_ref()?;
    if left.coverage != Coverage::Complete
        || right.coverage != Coverage::Complete
        || !left.limits_hit.is_empty()
        || !right.limits_hit.is_empty()
    {
        return None;
    }
    let before = guards(left)?;
    let after = guards(right)?;
    if before.len() != after.len() {
        return None;
    }
    let mut changed = Vec::new();
    for (a, b) in before.iter().zip(&after) {
        if relative_lines(a.span.as_ref()?, left)? != relative_lines(b.span.as_ref()?, right)? {
            return None;
        }
        let old = input_guard(&a.condition, left)?;
        let new = input_guard(&b.condition, right)?;
        for slot in [old.0, new.0] {
            if !full.analysis.input_correspondence.iter().any(|pair| {
                pair.position == slot
                    && left.parameters.get(slot as usize) == Some(&pair.before)
                    && right.parameters.get(slot as usize) == Some(&pair.after)
            }) {
                return None;
            }
        }
        if old != new {
            changed.push((*a, *b, old, new));
        }
    }
    let [(a, b, old, new)] = changed.as_slice() else {
        return None;
    };
    // Existing inversion explanations retain their established presentation.
    if old.0 == new.0 || observation_shapes(left, &before)? != observation_shapes(right, &after)? {
        return None;
    }
    let (old_source, expression, before_regions) = selected_source(
        &full.analysis.before,
        &compact.before_flow,
        &a.branch,
        &compact.input_names,
    )?;
    let (new_source, after_expression, after_regions) = selected_source(
        &full.analysis.after,
        &compact.after_flow,
        &b.branch,
        &compact.input_names,
    )?;
    if old_source != new_source || expression != after_expression || before_regions == after_regions
    {
        return None;
    }
    let label = |key: InputGuard| -> Option<String> {
        Some(format!(
            "{}{}",
            if key.1 { "!" } else { "" },
            compact.input_names.get(key.0 as usize)?
        ))
    };
    let pair = (
        a.branch.clone(),
        b.branch.clone(),
        label(*old)?,
        label(*new)?,
    );
    let source = expression
        .strip_prefix('(')
        .and_then(|text| text.strip_suffix(')'))
        .unwrap_or(&expression);
    let lines = vec![
        format!("Control: {} -> {}", pair.2, pair.3),
        format!("Return source: {source}"),
        format!(
            "  Before: under {}",
            regions_text(&before_regions, &compact.input_names)
        ),
        format!(
            "  After: under {}",
            regions_text(&after_regions, &compact.input_names)
        ),
    ];
    Some(ReturnSelectionRule { pair, lines })
}
