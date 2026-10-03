//! Text-only policy; serialized presentation and evidence remain unchanged.

use super::*;
use crate::ifds::ir::PrimitiveOperator;
use crate::ifds::model::{NodeId, Source};

pub(super) type GuardPair = (NodeId, NodeId, String, String);

pub(super) fn dependency_edit_lines(
    compact: &FunctionResultCompactReport,
    full: &FunctionResultReport,
) -> Vec<String> {
    let (Some(left), Some(right)) = (&full.analysis.before, &full.analysis.after) else {
        return Vec::new();
    };
    let mut shown = BTreeSet::new();
    let mut lines = Vec::new();
    for finding in &full.findings {
        if !compact.findings.iter().any(|visible| {
            visible
                .evidence
                .iter()
                .any(|evidence| finding.regions.contains(&evidence.index))
        }) {
            continue;
        }
        let Some(edit) = unique_dependency_edit(&full.analysis, &full.comparison, &finding.regions)
        else {
            continue;
        };
        let (Some((before, _)), Some((after, _)), Some(a), Some(b)) = (
            dependency_expression(edit.before, left, &compact.input_names),
            dependency_expression(edit.after, right, &compact.input_names),
            &edit.before.span,
            &edit.after.span,
        ) else {
            continue;
        };
        if shown.insert((&edit.before.node, &edit.after.node)) {
            lines.push(format!(
                "Edit: {before} -> {after} (before {}:{}; after {}:{})",
                a.path, a.start_line, b.path, b.start_line
            ));
        }
    }
    lines
}

/// Expand resolved value dependencies for display, without evaluating them or
/// inferring runtime types. Input names use paired positions across snapshots.
pub(super) fn dependency_expression(
    dependency: &ResultDependency,
    side: &FunctionSnapshotResult,
    names: &[String],
) -> Option<(String, bool)> {
    dependency_expression_with_replacement(dependency, side, names, None)
}

pub(super) fn dependency_expression_with_replacement(
    dependency: &ResultDependency,
    side: &FunctionSnapshotResult,
    names: &[String],
    replacement: Option<(&NodeId, &str)>,
) -> Option<(String, bool)> {
    if dependency.unresolved {
        return None;
    }
    if let Some((node, label)) = replacement
        && dependency.node == *node
    {
        return Some((label.into(), false));
    }
    if let Some(Source::FunctionInput(binding)) = &dependency.origin {
        let slot = side.parameters.iter().position(|item| item == binding)?;
        return Some((names.get(slot)?.clone(), false));
    }
    if let Some(literal) = &dependency.literal {
        return Some((primitive_text(literal), false));
    }
    let inputs = dependency
        .inputs
        .iter()
        .map(|input| dependency_expression_with_replacement(input, side, names, replacement))
        .collect::<Option<Vec<_>>>()?;
    if transparent(dependency) {
        let first = inputs.first()?;
        return inputs
            .iter()
            .all(|input| input == first)
            .then(|| first.clone());
    }
    let operator = dependency.operator.as_ref()?;
    let expression = match (operator, inputs.as_slice()) {
        (PrimitiveOperator::UnaryPlus, [(input, _)]) => format!("(+{input})"),
        (PrimitiveOperator::UnaryMinus, [(input, _)]) => format!("(-{input})"),
        (PrimitiveOperator::LogicalNot, [(input, _)]) => format!("(!{input})"),
        (_, [(left, _), (right, _)]) => {
            let symbol = match operator {
                PrimitiveOperator::Add => "+",
                PrimitiveOperator::Subtract => "-",
                PrimitiveOperator::Multiply => "*",
                PrimitiveOperator::Divide => "/",
                PrimitiveOperator::Remainder => "%",
                PrimitiveOperator::StrictEqual => "===",
                PrimitiveOperator::StrictNotEqual => "!==",
                PrimitiveOperator::GreaterThan => ">",
                PrimitiveOperator::LessThan => "<",
                _ => return None,
            };
            format!("({left} {symbol} {right})")
        }
        _ => return None,
    };
    Some((expression, true))
}

pub(super) fn changed_computation_lines(
    full: &FunctionResultReport,
    region: &ComparedResultRegion,
    names: &[String],
) -> Vec<String> {
    if !region.value_dependency_changed
        || full.comparison.input_mapping_coverage != Coverage::Complete
    {
        return Vec::new();
    }
    let (Some((before_key, (before, before_compute))), Some((after_key, (after, after_compute)))) = (
        resolved_computation(&full.analysis.before, &region.before_observations, names),
        resolved_computation(&full.analysis.after, &region.after_observations, names),
    ) else {
        return Vec::new();
    };
    if before_key == after_key || before == after || !(before_compute || after_compute) {
        return Vec::new();
    }
    let mut lines = vec![
        format!(
            "Changed computation under {}:",
            region_text(&region.region, names)
        ),
        format!("  {before} -> {after} ({:?})", region.assessment),
    ];
    if let (Some(before), Some(after)) = (&region.before_result, &region.after_result) {
        lines.push(if before == after {
            format!("  Reaches return {before}")
        } else {
            format!("  Reaches return {before} -> return {after}")
        });
    }
    lines
}

fn resolved_computation(
    side: &Option<FunctionSnapshotResult>,
    refs: &[ResultObservationRef],
    names: &[String],
) -> Option<(String, (String, bool))> {
    let snapshot = side.as_ref()?;
    let mut expressions = BTreeSet::new();
    for reference in refs {
        let (_, item) = observation(side, reference)?;
        if item.dependency_coverage != Coverage::Complete || item.unknown_completion_before_return {
            return None;
        }
        expressions.insert(match &item.value {
            ResultValue::Undefined => ("undefined".into(), ("undefined".into(), false)),
            ResultValue::Expression { dependency, .. } => (
                dependency_key(dependency, snapshot)?,
                dependency_expression(dependency, snapshot, names)?,
            ),
        });
    }
    (expressions.len() == 1).then(|| expressions.into_iter().next().unwrap())
}

/// Value equality and dependency identity are separate claims. Only the type
/// theory may remain unresolved here; pairing, path selection and completion
/// must already be established by the comparison.
pub(super) fn unchanged_data_flow_line(
    full: &FunctionResultReport,
    region: &ComparedResultRegion,
    names: &[String],
) -> Option<String> {
    if full.comparison.input_mapping_coverage != Coverage::Complete
        || full.comparison.before_entry != full.comparison.after_entry
        || region.value_dependency_changed
        || !(region.assessment == ResultAssessment::Equal
            || matches!(
                &region.evidence,
                crate::ifds::model::EvidenceKind::Unresolved {
                    diagnostic: crate::ifds::model::DiagnosticCode::TypeResolutionUnavailable
                }
            ))
    {
        return None;
    }
    let before = resolved_computation(&full.analysis.before, &region.before_observations, names)?;
    let after = resolved_computation(&full.analysis.after, &region.after_observations, names)?;
    if before != after {
        return None;
    }
    let status = if region.assessment == ResultAssessment::Equal {
        "Equal"
    } else {
        "runtime value equality unresolved"
    };
    Some(format!(
        "Unchanged data flow under {}: {} ({status})",
        region_text(&region.region, names),
        before.1.0
    ))
}

pub(super) fn unchanged_data_flow_summary(
    compact: &FunctionResultCompactReport,
    full: &FunctionResultReport,
) -> (Vec<String>, BTreeSet<usize>) {
    // Omitted findings must not expand a summary's scope. A complete unchanged
    // runtime-result summary already covers the all-equal, unchanged-control case.
    if compact.coverage.presentation != Coverage::Complete
        || normal_results_unchanged(compact, full)
    {
        return (Vec::new(), BTreeSet::new());
    }
    let indices: BTreeSet<_> = compact
        .equal_regions
        .iter()
        .chain(&compact.unknown_regions)
        .filter_map(|region| {
            unchanged_data_flow_line(
                full,
                &full.comparison.regions[region.evidence.index],
                &compact.input_names,
            )
            .map(|_| region.evidence.index)
        })
        .collect();
    if indices.len() < 2 {
        return (Vec::new(), BTreeSet::new());
    }
    let regions = simplify_regions(
        indices
            .iter()
            .map(|index| full.comparison.regions[*index].region.clone())
            .collect(),
    );
    let unknown_count = indices
        .iter()
        .filter(|index| full.comparison.regions[**index].assessment == ResultAssessment::Unknown)
        .count();
    let status = if unknown_count == 0 {
        "Equal"
    } else if unknown_count == indices.len() {
        "runtime value equality unresolved"
    } else {
        "runtime value equality unresolved in some regions"
    };
    (
        vec![format!(
            "Unchanged data flow under {} ({status})",
            regions_text(&regions, &compact.input_names)
        )],
        indices,
    )
}

pub(super) fn conditional_choices(finding: &CompactResultFinding) -> bool {
    finding.effects.len() > 1
        || finding.choices.iter().any(|choice| {
            choice
                .before
                .iter()
                .chain(&choice.after)
                .any(|region| !region.values.is_empty())
        })
}

pub(super) fn normal_results_unchanged(
    compact: &FunctionResultCompactReport,
    full: &FunctionResultReport,
) -> bool {
    compact.function_presence == FunctionPresence::Both
        && compact.common_domain_status == CommonDomainStatus::Present
        && !compact.common_domain.is_empty()
        && compact.unresolved_domain.is_empty()
        && compact.coverage.result_comparison == Coverage::Complete
        && compact.coverage.input_mapping == Coverage::Complete
        && compact.coverage.presentation == Coverage::Complete
        && compact.coverage.before_analysis == Some(Coverage::Complete)
        && compact.coverage.after_analysis == Some(Coverage::Complete)
        && compact.coverage.analysis_limits_hit.is_empty()
        && compact.coverage.comparison_limits_hit.is_empty()
        && !full.comparison.regions.is_empty()
        && full
            .comparison
            .regions
            .iter()
            .all(|region| region.assessment == ResultAssessment::Equal && !region.control_changed)
}

/// Establish one changed guard through unique paired-input value dependencies.
/// Branch order, expression labels, and result equality alone are insufficient.
pub(super) fn changed_guard_pair(full: &FunctionResultReport) -> Option<GuardPair> {
    if full.analysis.input_mapping_coverage != Coverage::Complete {
        return None;
    }
    let before = full.analysis.before.as_ref()?;
    let after = full.analysis.after.as_ref()?;
    fn guards(side: &FunctionSnapshotResult) -> Option<BTreeMap<(u32, bool), &ResultGuard>> {
        let mut branches: BTreeMap<&NodeId, &ResultGuard> = BTreeMap::new();
        for guard in side.observations.iter().flat_map(|item| &item.guards) {
            if let Some(previous) = branches.insert(&guard.branch, guard)
                && input_guard(&previous.condition, side) != input_guard(&guard.condition, side)
            {
                return None;
            }
        }
        let mut guards = BTreeMap::new();
        for guard in branches.into_values() {
            let key = input_guard(&guard.condition, side)?;
            if guards.insert(key, guard).is_some() {
                return None;
            }
        }
        Some(guards)
    }
    let left = guards(before)?;
    let right = guards(after)?;
    let removed: Vec<_> = left.keys().filter(|key| !right.contains_key(key)).collect();
    let added: Vec<_> = right.keys().filter(|key| !left.contains_key(key)).collect();
    let ([old], [new]) = (removed.as_slice(), added.as_slice()) else {
        return None;
    };
    if old.0 != new.0 || old.1 == new.1 {
        return None;
    }
    if !full.analysis.input_correspondence.iter().any(|pair| {
        pair.position == old.0
            && before.parameters.get(old.0 as usize) == Some(&pair.before)
            && after.parameters.get(new.0 as usize) == Some(&pair.after)
    }) {
        return None;
    }
    // Anchor the unique inversion to stable return-site structure, as in the
    // symmetric-effect proof. Independent removed/added guards must not acquire
    // correspondence merely by testing complementary values of the same input.
    fn ordered(side: &FunctionSnapshotResult) -> Vec<&ResultObservation> {
        let mut items: Vec<_> = side.observations.iter().collect();
        items.sort_by_key(|item| (item.span.byte_start, item.path_index));
        items
    }
    let left_items = ordered(before);
    let right_items = ordered(after);
    if left_items.len() != right_items.len() {
        return None;
    }
    for (left_item, right_item) in left_items.iter().zip(&right_items) {
        let affected_before = left_item
            .guards
            .iter()
            .any(|guard| guard.branch == left[*old].branch);
        let affected_after = right_item
            .guards
            .iter()
            .any(|guard| guard.branch == right[*new].branch);
        if affected_before != affected_after {
            return None;
        }
        if !affected_before {
            continue;
        }
        let value_key = |item: &ResultObservation, side: &FunctionSnapshotResult| match &item.value
        {
            ResultValue::Undefined => Some("undefined".into()),
            ResultValue::Expression { dependency, .. } => dependency_key(dependency, side),
        };
        let guard_keys =
            |item: &ResultObservation, side: &FunctionSnapshotResult, replace: bool| {
                item.guards
                    .iter()
                    .map(|guard| {
                        let key = input_guard(&guard.condition, side)?;
                        Some((
                            if replace && key == **new { **old } else { key },
                            guard.outcome,
                        ))
                    })
                    .collect::<Option<BTreeSet<_>>>()
            };
        if left_item.kind != right_item.kind
            || value_key(left_item, before).is_none()
            || value_key(left_item, before) != value_key(right_item, after)
            || guard_keys(left_item, before, false) != guard_keys(right_item, after, true)
        {
            return None;
        }
    }
    fn label(side: &FunctionSnapshotResult, guard: &ResultGuard, key: (u32, bool)) -> String {
        let name = &side.parameter_names[key.0 as usize];
        let resolved = if key.1 {
            format!("!{name}")
        } else {
            name.clone()
        };
        // A sole positive guard has an unambiguous recorded source label. For
        // nested guards, print the resolved dependency explicitly rather than
        // guessing how the joined path-condition text partitions into guards.
        let labels: BTreeSet<_> = side
            .observations
            .iter()
            .filter_map(|item| {
                (item.guards.len() == 1
                    && item.guards[0].branch == guard.branch
                    && item.guards[0].outcome)
                    .then_some(item.effective_condition.as_ref())
                    .flatten()
            })
            .collect();
        if labels.len() == 1 {
            let source = *labels.first().unwrap();
            if source == &resolved {
                source.clone()
            } else {
                format!("{source} (resolved: {resolved})")
            }
        } else {
            format!("resolved: {resolved}")
        }
    }
    Some((
        left[*old].branch.clone(),
        right[*new].branch.clone(),
        label(before, left[*old], **old),
        label(after, right[*new], **new),
    ))
}

pub(super) fn equal_control_lines(
    compact: &FunctionResultCompactReport,
    full: &FunctionResultReport,
    pair: Option<&GuardPair>,
    verbose: bool,
    shown_controls: &mut BTreeSet<String>,
    grouped_unchanged: &BTreeSet<usize>,
) -> Vec<String> {
    let mut lines = Vec::new();
    for finding in &full.findings {
        if finding.assessment != ResultAssessment::Equal || !finding.control_changed {
            continue;
        }
        // Equal control findings suppressed from JSON alongside changed results
        // are still available through equal_regions. Under a presentation limit,
        // only emit regions whose compact finding survived that limit.
        let mut regions: Vec<_> = compact
            .equal_regions
            .iter()
            .filter(|region| {
                finding.regions.contains(&region.evidence.index)
                    && (compact.coverage.presentation == Coverage::Complete
                        || compact.findings.iter().any(|visible| {
                            visible.assessment == ResultAssessment::Equal
                                && visible.evidence.contains(&region.evidence)
                        }))
            })
            .collect();
        if regions.is_empty() {
            continue;
        }
        if regions
            .iter()
            .all(|region| grouped_unchanged.contains(&region.evidence.index))
        {
            let mut control = control_text(full, finding.regions.iter().copied(), pair);
            if guard_order_text(full, finding.regions.iter().copied()).is_none() {
                control.push_str(&format!(
                    " under {} in the common input scope",
                    regions_text(
                        &simplify_regions(
                            regions.iter().map(|region| region.region.clone()).collect()
                        ),
                        &compact.input_names
                    )
                ));
                if !finding.attribution_certain {
                    control.push_str("; Edit attribution uncertain");
                }
            }
            if shown_controls.insert(control.clone()) {
                lines.push(control);
            }
            continue;
        }
        regions.retain(|region| !grouped_unchanged.contains(&region.evidence.index));
        if let Some(control) = guard_order_text(full, finding.regions.iter().copied()) {
            if shown_controls.insert(control.clone()) {
                lines.push(control);
            }
            for region in regions {
                let compared = &full.comparison.regions[region.evidence.index];
                if let Some(line) = unchanged_data_flow_line(full, compared, &compact.input_names) {
                    lines.push(line);
                } else {
                    lines.push(format!(
                        "Equal results under {}: {} / {}",
                        region_text(&region.region, &compact.input_names),
                        region
                            .before_result
                            .as_ref()
                            .map_or_else(|| "unknown".into(), PresentedResult::display),
                        region
                            .after_result
                            .as_ref()
                            .map_or_else(|| "unknown".into(), PresentedResult::display),
                    ));
                }
            }
            continue;
        }
        let mut results: BTreeMap<(String, String), Vec<BooleanRegion>> = BTreeMap::new();
        for region in regions {
            let display = |result: &Option<PresentedResult>| {
                result
                    .as_ref()
                    .map_or_else(|| "unknown".into(), PresentedResult::display)
            };
            results
                .entry((
                    display(&region.before_result),
                    display(&region.after_result),
                ))
                .or_default()
                .push(region.region.clone());
        }
        let result_text = if !verbose && results.len() > 1 {
            let regions = results.values().flatten().cloned().collect();
            format!(
                "results unchanged (Equal) under {} in the common input scope",
                regions_text(&simplify_regions(regions), &compact.input_names)
            )
        } else {
            results
                .into_iter()
                .map(|((before, after), regions)| {
                    let scope = regions_text(&simplify_regions(regions), &compact.input_names);
                    let result = if before == after {
                        format!("result remains {before} (Equal)")
                    } else {
                        format!("equal results: {before} / {after} (Equal)")
                    };
                    format!("{result} under {scope} in the common input scope")
                })
                .collect::<Vec<_>>()
                .join("; ")
        };
        let control = control_text(full, finding.regions.iter().copied(), pair);
        let attribution = if finding.attribution_certain {
            ""
        } else {
            "; Edit attribution uncertain"
        };
        lines.push(format!("{control}; {result_text}{attribution}"));
    }
    lines
}

pub(super) fn control_text(
    full: &FunctionResultReport,
    regions: impl Iterator<Item = usize>,
    pair: Option<&GuardPair>,
) -> String {
    let regions: Vec<_> = regions.collect();
    if let Some(order) = guard_order_text(full, regions.iter().copied()) {
        return order;
    }
    // A changed guard must contribute to the displayed regions, rather than
    // merely occur elsewhere in the function.
    let contributes =
        |side: &Option<FunctionSnapshotResult>, refs: &[ResultObservationRef], branch: &NodeId| {
            !refs.is_empty()
                && refs.iter().all(|reference| {
                    observation(side, reference).is_some_and(|(_, item)| {
                        item.guards.iter().any(|guard| &guard.branch == branch)
                    })
                })
        };
    if let Some((before, after, before_label, after_label)) = pair
        && regions.into_iter().all(|index| {
            let region = &full.comparison.regions[index];
            contributes(&full.analysis.before, &region.before_observations, before)
                && contributes(&full.analysis.after, &region.after_observations, after)
        })
    {
        format!("Control: {before_label} -> {after_label}")
    } else {
        "Control changed (guard correspondence unresolved)".into()
    }
}

/// A permutation of unique, stable paired-input guards can be described without
/// matching branches by source position. Guard versions and outcomes must match.
fn guard_order_text(
    full: &FunctionResultReport,
    regions: impl Iterator<Item = usize>,
) -> Option<String> {
    if full.comparison.input_mapping_coverage != Coverage::Complete {
        return None;
    }
    let left = full.analysis.before.as_ref()?;
    let right = full.analysis.after.as_ref()?;
    let mut orders = BTreeSet::new();
    for index in regions {
        let region = &full.comparison.regions[index];
        let ([left_ref], [right_ref]) = (
            region.before_observations.as_slice(),
            region.after_observations.as_slice(),
        ) else {
            return None;
        };
        let (_, before) = observation(&full.analysis.before, left_ref)?;
        let (_, after) = observation(&full.analysis.after, right_ref)?;
        let sequence = |item: &ResultObservation, side: &FunctionSnapshotResult| {
            item.guards
                .iter()
                .map(|guard| {
                    let (slot, inverted) = input_guard(&guard.condition, side)?;
                    Some((slot, inverted, guard.outcome))
                })
                .collect::<Option<Vec<_>>>()
        };
        let before = sequence(before, left)?;
        let after = sequence(after, right)?;
        let before_set: BTreeSet<_> = before.iter().copied().collect();
        let after_set: BTreeSet<_> = after.iter().copied().collect();
        if before == after || before_set != after_set || before_set.len() != before.len() {
            return None;
        }
        // Outcomes identify the selected path; only guard expressions describe
        // their evaluation order, so all four truthiness paths share one label.
        orders.insert((
            before
                .into_iter()
                .map(|(slot, inverted, _)| (slot, inverted))
                .collect::<Vec<_>>(),
            after
                .into_iter()
                .map(|(slot, inverted, _)| (slot, inverted))
                .collect::<Vec<_>>(),
        ));
    }
    if orders.len() != 1 {
        return None;
    }
    let (before, after) = orders.into_iter().next()?;
    let label = |order: Vec<(u32, bool)>| -> Option<String> {
        order
            .into_iter()
            .map(|(slot, inverted)| {
                let name = left.parameter_names.get(slot as usize)?;
                Some(format!("{}{name}", if inverted { "!" } else { "" }))
            })
            .collect::<Option<Vec<_>>>()
            .map(|labels| labels.join(" then "))
    };
    Some(format!(
        "Control: guard order {} -> {}",
        label(before)?,
        label(after)?
    ))
}

pub(super) fn coverage_text(coverage: &FunctionReportCoverage) -> String {
    let mut fields = vec![
        format!("result comparison {:?}", coverage.result_comparison),
        format!("source alignment {:?}", coverage.source_alignment),
    ];
    for (name, status) in [
        ("before analysis", coverage.before_analysis),
        ("after analysis", coverage.after_analysis),
        ("input mapping", Some(coverage.input_mapping)),
        ("presentation", Some(coverage.presentation)),
    ] {
        if let Some(status) = status.filter(|status| *status != Coverage::Complete) {
            fields.push(format!("{name} {status:?}"));
        }
    }
    if coverage.presentation != Coverage::Complete || coverage.omitted_groups != 0 {
        fields.push(format!("{} groups omitted", coverage.omitted_groups));
    }
    for (name, limits) in [
        ("analysis limits", &coverage.analysis_limits_hit),
        ("comparison limits", &coverage.comparison_limits_hit),
    ] {
        if !limits.is_empty() {
            fields.push(format!(
                "{name}: {}",
                limits.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
    }
    format!("Coverage: {}", fields.join("; "))
}
