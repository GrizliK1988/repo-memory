//! Text-only policy; serialized presentation and evidence remain unchanged.

use super::*;
use crate::ifds::model::NodeId;

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
fn changed_guard_pair(full: &FunctionResultReport) -> Option<(NodeId, NodeId, String, String)> {
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
) -> Vec<String> {
    let pair = changed_guard_pair(full);
    let mut lines = Vec::new();
    for finding in &full.findings {
        if finding.assessment != ResultAssessment::Equal || !finding.control_changed {
            continue;
        }
        // Equal control findings suppressed from JSON alongside changed results
        // are still available through equal_regions. Under a presentation limit,
        // only emit regions whose compact finding survived that limit.
        let regions: Vec<_> = compact
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
        let result_text = results
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
            .join("; ");
        // The paired guard must contribute to this finding, not just occur
        // elsewhere in the function. A conservative fallback is preferable to
        // attributing an unrelated global guard change to a selected return.
        let contributes = |side: &Option<FunctionSnapshotResult>,
                           refs: &[ResultObservationRef],
                           branch: &NodeId| {
            !refs.is_empty()
                && refs.iter().all(|reference| {
                    observation(side, reference).is_some_and(|(_, item)| {
                        item.guards.iter().any(|guard| &guard.branch == branch)
                    })
                })
        };
        let relevant_pair = pair.as_ref().filter(|(before, after, _, _)| {
            finding.regions.iter().all(|index| {
                let region = &full.comparison.regions[*index];
                contributes(&full.analysis.before, &region.before_observations, before)
                    && contributes(&full.analysis.after, &region.after_observations, after)
            })
        });
        let control = if let Some((_, _, before, after)) = relevant_pair {
            format!("Control: {before} -> {after}")
        } else {
            "Control changed (guard correspondence unresolved)".into()
        };
        let attribution = if finding.attribution_certain {
            ""
        } else {
            "; Edit attribution uncertain"
        };
        lines.push(format!("{control}; {result_text}{attribution}"));
    }
    lines
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
