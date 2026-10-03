//! Short text for one changed dependency shared by several return paths.

use super::*;
use crate::ifds::model::NodeId;

#[derive(PartialEq, Eq)]
struct SharedDependency<'a> {
    label: &'a str,
    before_key: String,
    after_key: String,
    before_expression: String,
    after_expression: String,
    before_writes: BTreeMap<NodeId, SourceSpan>,
    after_writes: BTreeMap<NodeId, SourceSpan>,
}

fn root(observation: &ResultObservation) -> Option<&ResultDependency> {
    match &observation.value {
        ResultValue::Expression { dependency, .. } => Some(dependency),
        ResultValue::Undefined => None,
    }
}

fn shared_read<'a>(
    observation: &'a ResultObservation,
    edit: &NodeId,
) -> Option<(&'a ResultDependency, &'a str)> {
    fn contains(dependency: &ResultDependency, node: &NodeId) -> bool {
        dependency.node == *node || dependency.inputs.iter().any(|input| contains(input, node))
    }
    fn collect<'a>(
        dependency: &'a ResultDependency,
        edit: &NodeId,
        span: &SourceSpan,
        text: &'a str,
        candidates: &mut Vec<(&'a ResultDependency, &'a str)>,
    ) {
        if dependency.operation == "read"
            && let Some(location) = &dependency.span
            && location.path == span.path
            && location.byte_start >= span.byte_start
            && location.byte_end <= span.byte_end
            && contains(dependency, edit)
            && let Some(label) = text.get(
                (location.byte_start - span.byte_start) as usize
                    ..(location.byte_end - span.byte_start) as usize,
            )
        {
            candidates.push((dependency, label));
        }
        for input in &dependency.inputs {
            collect(input, edit, span, text, candidates);
        }
    }
    let ResultValue::Expression {
        text,
        span,
        dependency,
    } = &observation.value
    else {
        return None;
    };
    let mut candidates = Vec::new();
    collect(dependency, edit, span, text, &mut candidates);
    if candidates.len() == 1 {
        candidates.pop()
    } else {
        None
    }
}

fn write_locations(spans: &BTreeMap<NodeId, SourceSpan>) -> String {
    if spans.is_empty() {
        return "none".into();
    }
    spans
        .values()
        .map(|span| (&span.path, span.start_line))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|(path, line)| format!("{path}:{line}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn shared_finding_lines(
    compact: &FunctionResultCompactReport,
    full: &FunctionResultReport,
    finding: &FunctionResultFinding,
) -> Option<Vec<String>> {
    let edit = unique_dependency_edit(&full.analysis, &full.comparison, &finding.regions)?;
    let left = full.analysis.before.as_ref()?;
    let right = full.analysis.after.as_ref()?;
    let mut shared: Option<SharedDependency<'_>> = None;
    let mut contexts = Vec::new();
    for &index in &finding.regions {
        let region = &full.comparison.regions[index];
        let ([a], [b]) = (
            region.before_observations.as_slice(),
            region.after_observations.as_slice(),
        ) else {
            return None;
        };
        let (_, a) = observation(&full.analysis.before, a)?;
        let (_, b) = observation(&full.analysis.after, b)?;
        let (before, label) = shared_read(a, &edit.before.node)?;
        let (after, after_label) = shared_read(b, &edit.after.node)?;
        if label != after_label {
            return None;
        }
        let before_key = dependency_key(before, left)?;
        let after_key = dependency_key(after, right)?;
        if before_key == after_key {
            return None;
        }
        let (before_expression, _) = dependency_expression(before, left, &compact.input_names)?;
        let (after_expression, _) = dependency_expression(after, right, &compact.input_names)?;
        let before_writes = key_write_spans(before);
        let after_writes = key_write_spans(after);
        let signature = SharedDependency {
            label,
            before_key,
            after_key,
            before_expression,
            after_expression,
            before_writes,
            after_writes,
        };
        if let Some(previous) = &shared {
            if previous != &signature {
                return None;
            }
        } else {
            shared = Some(signature);
        }
        let (before_context, _) = dependency_expression_with_replacement(
            root(a)?,
            left,
            &compact.input_names,
            Some((&before.node, label)),
        )?;
        let (after_context, _) = dependency_expression_with_replacement(
            root(b)?,
            right,
            &compact.input_names,
            Some((&after.node, label)),
        )?;
        if before_context != after_context {
            return None;
        }
        let mut before_context_writes = key_write_spans(root(a)?);
        let mut after_context_writes = key_write_spans(root(b)?);
        let common = shared.as_ref()?;
        before_context_writes.retain(|node, _| !common.before_writes.contains_key(node));
        after_context_writes.retain(|node, _| !common.after_writes.contains_key(node));
        contexts.push((
            region_text(&region.region, &compact.input_names),
            before_context,
            before_context_writes,
            after_context_writes,
        ));
    }
    let SharedDependency {
        label,
        before_expression: before,
        after_expression: after,
        before_writes,
        after_writes,
        ..
    } = shared?;
    let mut lines = vec![
        format!("Changed computation for {label}:"),
        format!("  {before} -> {after} (Changed)"),
        format!("  Reaches return {}", finding.before_result.as_ref()?),
    ];
    for (side, writes) in [("Before", before_writes), ("After", after_writes)] {
        if !writes.is_empty() {
            lines.push(format!(
                "{side} shared writes: {}",
                write_locations(&writes)
            ));
        }
    }
    for (condition, context, before_writes, after_writes) in contexts {
        lines.push(format!("Return under {condition}: {context}"));
        if !before_writes.is_empty() || !after_writes.is_empty() {
            lines.push(format!(
                "  Context writes: before {}; after {}",
                write_locations(&before_writes),
                write_locations(&after_writes),
            ));
        }
    }
    Some(lines)
}

pub(super) fn shared_computation_lines(
    compact: &FunctionResultCompactReport,
    full: &FunctionResultReport,
) -> (Vec<String>, BTreeSet<usize>) {
    let visible: BTreeSet<_> = compact
        .findings
        .iter()
        .flat_map(|finding| &finding.evidence)
        .map(|reference| reference.index)
        .collect();
    let mut lines = Vec::new();
    let mut grouped = BTreeSet::new();
    for finding in &full.findings {
        if finding.assessment != ResultAssessment::Changed
            || finding.control_changed
            || finding.regions.len() < 2
            || finding.before_result != finding.after_result
            || !finding.regions.iter().all(|index| visible.contains(index))
            || compact.findings.iter().any(|visible| {
                visible
                    .evidence
                    .iter()
                    .any(|reference| finding.regions.contains(&reference.index))
                    && visible
                        .evidence
                        .iter()
                        .any(|reference| !finding.regions.contains(&reference.index))
            })
        {
            continue;
        }
        if let Some(shared) = shared_finding_lines(compact, full, finding) {
            lines.extend(shared);
            grouped.extend(&finding.regions);
        }
    }
    (lines, grouped)
}
