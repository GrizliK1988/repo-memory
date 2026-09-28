//! Presentation facts derived conservatively from full result evidence.

use super::*;
use crate::ifds::ir::PrimitiveOperator;
use crate::ifds::model::{NodeId, Source};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyEvidence {
    /// Child indices starting at the observation's result dependency root.
    pub path: Vec<usize>,
    pub node: NodeId,
    pub operation: String,
    pub span: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReturnEvidence {
    pub observation: EvidenceRef,
    pub return_location: SourceSpan,
    pub dependencies: Vec<DependencyEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentedResult {
    pub expression: String,
    pub proven_value: Option<FunctionKnownValue>,
    /// Distinguishes symbolic values supplied by different inputs/dependencies.
    pub identity: String,
    pub evidence: Vec<ReturnEvidence>,
}

fn primitive_text(value: &FunctionKnownValue) -> String {
    match value {
        FunctionKnownValue::Boolean { value } => value.to_string(),
        FunctionKnownValue::Number { value } => value.clone(),
        FunctionKnownValue::String { value } => serde_json::to_string(value).unwrap(),
        FunctionKnownValue::Null => "null".into(),
        FunctionKnownValue::Undefined => "undefined".into(),
    }
}

impl PresentedResult {
    pub fn display(&self) -> String {
        let Some(value) = &self.proven_value else {
            return self.expression.clone();
        };
        // A direct literal has no contributing copy or computation to explain.
        let direct = !self.evidence.is_empty()
            && self.evidence.iter().all(|evidence| {
                (evidence.dependencies.is_empty() && *value == FunctionKnownValue::Undefined)
                    || (evidence.dependencies.len() == 1
                        && evidence.dependencies[0].path.is_empty()
                        && evidence.dependencies[0].operation == "literal")
            });
        if direct {
            self.expression.clone()
        } else {
            format!("{} ({})", self.expression, primitive_text(value))
        }
    }
}

fn supported_literal(value: &FunctionKnownValue) -> bool {
    match value {
        FunctionKnownValue::Number { value } => value
            .replace('_', "")
            .parse::<f64>()
            .is_ok_and(|n| n.is_finite() && n.to_bits() != (-0.0_f64).to_bits()),
        _ => true,
    }
}

fn transparent(dependency: &ResultDependency) -> bool {
    matches!(
        dependency.operation.as_str(),
        "read" | "write" | "binding_value"
    ) || matches!(
        dependency.operator,
        Some(PrimitiveOperator::ValueJoin { .. })
    )
}

fn known_value(
    dependency: &ResultDependency,
    side: &FunctionSnapshotResult,
) -> Option<FunctionKnownValue> {
    if dependency.unresolved {
        return None;
    }
    let value = if let Some(Source::FunctionInput(binding)) = &dependency.origin {
        let slot = side.parameters.iter().position(|item| item == binding)? as u32;
        side.entry.known_values.get(&slot)?.clone()
    } else if let Some(value) = &dependency.literal {
        value.clone()
    } else if transparent(dependency) {
        let mut inputs = dependency.inputs.iter();
        let first = known_value(inputs.next()?, side)?;
        if !inputs.all(|input| known_value(input, side).as_ref() == Some(&first)) {
            return None;
        }
        first
    } else {
        return None;
    };
    supported_literal(&value).then_some(value)
}

fn dependency_key(dependency: &ResultDependency, side: &FunctionSnapshotResult) -> Option<String> {
    if dependency.unresolved {
        return None;
    }
    if let Some(value) = known_value(dependency, side) {
        return Some(serde_json::to_string(&value).unwrap());
    }
    if let Some(Source::FunctionInput(binding)) = &dependency.origin {
        let slot = side.parameters.iter().position(|item| item == binding)?;
        return Some(format!("input:{slot}"));
    }
    let inputs = dependency
        .inputs
        .iter()
        .map(|input| dependency_key(input, side))
        .collect::<Option<Vec<_>>>()?;
    if transparent(dependency) {
        let first = inputs.first()?;
        return inputs
            .iter()
            .all(|input| input == first)
            .then(|| first.clone());
    }
    dependency.operator.as_ref().map(|operator| {
        serde_json::to_string(&(operator, inputs)).expect("dependency key serializes")
    })
}

fn contributing_dependencies(
    dependency: &ResultDependency,
    path: &mut Vec<usize>,
    output: &mut Vec<DependencyEvidence>,
) {
    if dependency.operation == "write"
        || dependency.literal.is_some()
        || matches!(dependency.origin, Some(Source::FunctionInput(_)))
    {
        output.push(DependencyEvidence {
            path: path.clone(),
            node: dependency.node.clone(),
            operation: dependency.operation.clone(),
            span: dependency.span.clone(),
        });
    }
    for (index, input) in dependency.inputs.iter().enumerate() {
        path.push(index);
        contributing_dependencies(input, path, output);
        path.pop();
    }
}

pub(super) fn present_result(
    id: &str,
    section: &str,
    side: &Option<FunctionSnapshotResult>,
    refs: &[ResultObservationRef],
    text: &Option<String>,
) -> Option<PresentedResult> {
    let expression = text.clone()?;
    let side = side.as_ref()?;
    let mut results = Vec::new();
    for reference in refs {
        let (index, item) = side.observations.iter().enumerate().find(|(_, item)| {
            item.site == reference.site && item.path_index == reference.path_index
        })?;
        let complete = item.dependency_coverage == Coverage::Complete
            && !item.unknown_completion_before_return;
        let (value, key, dependencies) = match &item.value {
            ResultValue::Undefined => (
                complete.then_some(FunctionKnownValue::Undefined),
                Some("undefined".into()),
                Vec::new(),
            ),
            ResultValue::Expression { dependency, .. } => {
                let mut dependencies = Vec::new();
                contributing_dependencies(dependency, &mut Vec::new(), &mut dependencies);
                (
                    complete.then(|| known_value(dependency, side)).flatten(),
                    dependency_key(dependency, side),
                    dependencies,
                )
            }
        };
        // An unresolved value must not acquire cross-snapshot identity from text.
        let key = key.unwrap_or_else(|| format!("unresolved:{:?}:{}", item.site, item.path_index));
        results.push((
            value,
            key,
            ReturnEvidence {
                observation: super::reference(id, section, index),
                return_location: item.span.clone(),
                dependencies,
            },
        ));
    }
    let (first_value, _, _) = results.first()?;
    let proven_value = results
        .iter()
        .all(|(value, _, _)| value == first_value)
        .then(|| first_value.clone())
        .flatten();
    let keys: BTreeSet<_> = results.iter().map(|(_, key, _)| key.clone()).collect();
    let identity = serde_json::to_string(&(&expression, &proven_value, keys)).unwrap();
    Some(PresentedResult {
        expression,
        proven_value,
        identity,
        evidence: results
            .into_iter()
            .map(|(_, _, evidence)| evidence)
            .collect(),
    })
}

pub(super) fn merge_result(target: &mut PresentedResult, source: &PresentedResult) {
    for evidence in &source.evidence {
        if !target.evidence.contains(evidence) {
            target.evidence.push(evidence.clone());
        }
    }
}

pub(super) fn compact_region(
    full: &FunctionResultReport,
    id: &str,
    index: usize,
) -> CompactResultRegion {
    let region = &full.comparison.regions[index];
    CompactResultRegion {
        region: region.region.clone(),
        before_result: present_result(
            id,
            "analysis.before.observations",
            &full.analysis.before,
            &region.before_observations,
            &region.before_result,
        ),
        after_result: present_result(
            id,
            "analysis.after.observations",
            &full.analysis.after,
            &region.after_observations,
            &region.after_result,
        ),
        assessment: region.assessment,
        reason: region.unknown_reason.clone(),
        evidence: super::reference(id, "comparison.regions", index),
    }
}

fn input_guard(
    dependency: &ResultDependency,
    side: &FunctionSnapshotResult,
) -> Option<(u32, bool)> {
    if dependency.unresolved {
        return None;
    }
    if let Some(Source::FunctionInput(binding)) = &dependency.origin {
        return side
            .parameters
            .iter()
            .position(|item| item == binding)
            .map(|slot| (slot as u32, false));
    }
    if dependency.inputs.len() != 1 {
        return None;
    }
    let (slot, inverted) = input_guard(&dependency.inputs[0], side)?;
    if dependency.operator == Some(PrimitiveOperator::LogicalNot) {
        Some((slot, !inverted))
    } else if transparent(dependency) {
        Some((slot, inverted))
    } else {
        None
    }
}

/// Establish a unique guard inversion using stable return-site order, unchanged
/// returned expressions, paired inputs, and the exact guard value dependencies.
/// Matching tables or complementary regions alone never establish this proof.
pub(super) fn symmetric_guard(
    full: &FunctionResultReport,
    a: &CompactResultRegion,
    b: &CompactResultRegion,
) -> bool {
    if a.assessment != ResultAssessment::Different || b.assessment != a.assessment {
        return false;
    }
    let (Some(ab), Some(aa), Some(bb), Some(ba)) = (
        &a.before_result,
        &a.after_result,
        &b.before_result,
        &b.after_result,
    ) else {
        return false;
    };
    if ab.identity != ba.identity || aa.identity != bb.identity || ab.identity == aa.identity {
        return false;
    }
    if a.region.values.keys().ne(b.region.values.keys()) {
        return false;
    }
    let flipped: Vec<_> = a
        .region
        .values
        .iter()
        .filter(|(slot, value)| b.region.values.get(slot) != Some(value))
        .map(|(slot, _)| *slot)
        .collect();
    if flipped.len() != 1 {
        return false;
    }
    let (Some(before), Some(after)) = (&full.analysis.before, &full.analysis.after) else {
        return false;
    };
    fn ordered(side: &FunctionSnapshotResult) -> Vec<&ResultObservation> {
        let mut observations: Vec<_> = side.observations.iter().collect();
        observations.sort_by_key(|item| (item.span.byte_start, item.path_index));
        observations
    }
    let before_items = ordered(before);
    let after_items = ordered(after);
    if before_items.len() != after_items.len() {
        return false;
    }
    let mut changed = BTreeSet::new();
    for (left, right) in before_items.iter().zip(&after_items) {
        if left.kind != right.kind
            || left.guards.len() != right.guards.len()
            || left.unknown_completion_before_return
            || right.unknown_completion_before_return
        {
            return false;
        }
        let value_key = |item: &ResultObservation, side: &FunctionSnapshotResult| match &item.value
        {
            ResultValue::Undefined => Some(("undefined".into(), "undefined".into())),
            ResultValue::Expression {
                text, dependency, ..
            } => dependency_key(dependency, side).map(|key| (text.clone(), key)),
        };
        if value_key(left, before).is_none() || value_key(left, before) != value_key(right, after) {
            return false;
        }
        for (left_guard, right_guard) in left.guards.iter().zip(&right.guards) {
            if left_guard.outcome != right_guard.outcome {
                return false;
            }
            let left_key = dependency_key(&left_guard.condition, before);
            let right_key = dependency_key(&right_guard.condition, after);
            if left_key.is_none() || right_key.is_none() {
                return false;
            }
            if left_key != right_key {
                let (Some((slot, neg)), Some((other_slot, other_neg))) = (
                    input_guard(&left_guard.condition, before),
                    input_guard(&right_guard.condition, after),
                ) else {
                    return false;
                };
                if slot != flipped[0] || other_slot != slot || neg == other_neg {
                    return false;
                }
                changed.insert((left_guard.branch.clone(), right_guard.branch.clone()));
            }
        }
    }
    if changed.len() != 1 {
        return false;
    }
    let (before_branch, after_branch) = changed.first().unwrap();
    // Every contributing return must pass through this particular changed guard.
    for (result, side, branch) in [
        (ab, before, before_branch),
        (bb, before, before_branch),
        (aa, after, after_branch),
        (ba, after, after_branch),
    ] {
        if result.evidence.is_empty()
            || !result.evidence.iter().all(|evidence| {
                side.observations[evidence.observation.index]
                    .guards
                    .iter()
                    .any(|guard| &guard.branch == branch)
            })
        {
            return false;
        }
    }
    true
}

pub(super) fn source_text(result: &PresentedResult) -> Vec<String> {
    let mut lines = BTreeSet::new();
    for evidence in &result.evidence {
        if !evidence
            .dependencies
            .iter()
            .any(|dependency| dependency.operation == "write")
        {
            continue;
        }
        let location = &evidence.return_location;
        let chain: Vec<_> = evidence
            .dependencies
            .iter()
            .filter(|dependency| dependency.operation == "write")
            .filter_map(|dependency| {
                dependency
                    .span
                    .as_ref()
                    .map(|span| format!("{}:{}", span.path, span.start_line))
            })
            .collect();
        if !chain.is_empty() {
            lines.insert(format!(
                "{:?} return {} at {}:{}; contributing writes: {}",
                evidence.dependencies[0].node.snapshot.side,
                result.expression,
                location.path,
                location.start_line,
                chain.join(", ")
            ));
        }
    }
    lines.into_iter().collect()
}
