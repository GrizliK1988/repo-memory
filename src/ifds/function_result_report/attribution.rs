//! Attribute a resolved result change to one source operation shared by its paths.

use super::*;
use crate::ifds::ir::PrimitiveOperator;
use crate::ifds::model::Source;

pub(super) struct DependencyEdit<'a> {
    pub before: &'a ResultDependency,
    pub after: &'a ResultDependency,
}

fn same_relative_lines(
    before: &SourceSpan,
    after: &SourceSpan,
    left: &FunctionSnapshotResult,
    right: &FunctionSnapshotResult,
) -> bool {
    before.path == left.function.declaration.path
        && after.path == right.function.declaration.path
        && before.start_line >= left.function.declaration.start_line
        && after.start_line >= right.function.declaration.start_line
        && before
            .start_line
            .checked_sub(left.function.declaration.start_line)
            == after
                .start_line
                .checked_sub(right.function.declaration.start_line)
        && before
            .end_line
            .checked_sub(left.function.declaration.start_line)
            == after
                .end_line
                .checked_sub(right.function.declaration.start_line)
}

fn dependency_edits<'a>(
    before: &'a ResultDependency,
    after: &'a ResultDependency,
    left: &FunctionSnapshotResult,
    right: &FunctionSnapshotResult,
) -> Option<Vec<DependencyEdit<'a>>> {
    if before.unresolved || after.unresolved || before.role != after.role {
        return None;
    }
    match (&before.span, &after.span) {
        (Some(a), Some(b)) if same_relative_lines(a, b, left, right) => {}
        (None, None) => {}
        _ => return None,
    }
    if let (Some(Source::FunctionInput(a)), Some(Source::FunctionInput(b))) =
        (&before.origin, &after.origin)
    {
        let a = left.parameters.iter().position(|binding| binding == a)?;
        let b = right.parameters.iter().position(|binding| binding == b)?;
        return Some(if a == b {
            Vec::new()
        } else {
            vec![DependencyEdit { before, after }]
        });
    }
    match (&before.origin, &after.origin) {
        (None, None) | (Some(Source::Write(_)), Some(Source::Write(_))) => {}
        _ => return None,
    }
    // Value joins carry snapshot-specific branch IDs, rather than operator edits.
    let operators_match = match (&before.operator, &after.operator) {
        (Some(PrimitiveOperator::ValueJoin { .. }), Some(PrimitiveOperator::ValueJoin { .. })) => {
            true
        }
        (a, b) => a == b,
    };
    if before.inputs.len() != after.inputs.len()
        || (before.operator.is_none() && before.operation != after.operation)
        || before.operator.is_some() != after.operator.is_some()
        || before.literal.is_some() != after.literal.is_some()
    {
        return None;
    }
    let mut edits = Vec::new();
    if !operators_match || before.literal != after.literal {
        edits.push(DependencyEdit { before, after });
    }
    for (a, b) in before.inputs.iter().zip(&after.inputs) {
        let mut children = dependency_edits(a, b, left, right)?;
        // Positional inputs have no source span. Anchor their changed selection
        // at the nearest read, where the operand was written in source code.
        for edit in &mut children {
            if edit.before.span.is_none()
                && edit.after.span.is_none()
                && before.span.is_some()
                && after.span.is_some()
                && before.operation == "read"
            {
                *edit = DependencyEdit { before, after };
            }
        }
        edits.extend(children);
    }
    Some(edits)
}

pub(super) fn unique_dependency_edit<'a>(
    analysis: &'a FunctionResultAnalysis,
    comparison: &FunctionResultComparison,
    regions: &[usize],
) -> Option<DependencyEdit<'a>> {
    if analysis.counterpart_status != FunctionCounterpart::Matched
        || analysis.input_mapping_coverage != Coverage::Complete
        || comparison.before_entry != comparison.after_entry
        || regions.is_empty()
    {
        return None;
    }
    let left = analysis.before.as_ref()?;
    let right = analysis.after.as_ref()?;
    let matching_return = |a: &ResultObservation, b: &ResultObservation| {
        a.kind == b.kind
            && same_relative_lines(&a.span, &b.span, left, right)
            && a.guards.len() == b.guards.len()
            && a.guards.iter().zip(&b.guards).all(|(a, b)| {
                a.outcome == b.outcome
                    && matches!((&a.span, &b.span), (Some(a), Some(b)) if same_relative_lines(a, b, left, right))
                    && dependency_key(&a.condition, left).is_some()
                    && dependency_key(&a.condition, left) == dependency_key(&b.condition, right)
            })
    };
    let mut unique: Option<DependencyEdit<'a>> = None;
    for &index in regions {
        let region = comparison.regions.get(index)?;
        if region.assessment == ResultAssessment::Unknown
            || region.control_changed
            || !region.value_dependency_changed
            || region.before_observations.is_empty()
            || region.before_observations.len() != region.after_observations.len()
        {
            return None;
        }
        let mut used = BTreeSet::new();
        for reference in &region.before_observations {
            let (_, a) = observation(&analysis.before, reference)?;
            let mut matches = region.after_observations.iter().filter_map(|reference| {
                let (index, b) = observation(&analysis.after, reference)?;
                matching_return(a, b).then_some((index, b))
            });
            let (index, b) = matches.next()?;
            if matches.next().is_some() || !used.insert(index) {
                return None;
            }
            if a.dependency_coverage != Coverage::Complete
                || b.dependency_coverage != Coverage::Complete
                || a.unknown_completion_before_return
                || b.unknown_completion_before_return
            {
                return None;
            }
            let (
                ResultValue::Expression { dependency: a, .. },
                ResultValue::Expression { dependency: b, .. },
            ) = (&a.value, &b.value)
            else {
                return None;
            };
            let mut edits: BTreeMap<_, _> = dependency_edits(a, b, left, right)?
                .into_iter()
                .map(|edit| ((&edit.before.node, &edit.after.node), edit))
                .collect();
            if edits.len() != 1 {
                return None;
            }
            let (_, edit) = edits.pop_first()?;
            if edit.before.span.is_none() || edit.after.span.is_none() {
                return None;
            }
            if let Some(previous) = &unique {
                if previous.before.node != edit.before.node
                    || previous.after.node != edit.after.node
                {
                    return None;
                }
            } else {
                unique = Some(edit);
            }
        }
    }
    unique
}
