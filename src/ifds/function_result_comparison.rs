//! Bounded same-input comparison of independently observed function results.

use super::function_results::*;
use super::ir::PrimitiveOperator;
use super::model::{Coverage, DiagnosticCode, EvidenceKind, NodeId, Source};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultAssessment {
    Equal,
    Different,
    /// Resolved result computation or source selection changed; runtime values
    /// need not be evaluated or proven unequal.
    Changed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommonDomainStatus {
    Present,
    Empty,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionPresence {
    Both,
    BeforeOnly,
    AfterOnly,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultProof {
    PrimitiveValues,
    PairedInputIdentity,
    PureExpressionIdentity,
    ChangedSourceSelection,
    ChangedComputation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BooleanRegion {
    /// A missing slot is unconstrained. Interpret this predicate together with
    /// the applicable before/after entry assumptions in the enclosing report.
    pub values: BTreeMap<u32, bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultObservationRef {
    pub site: NodeId,
    pub path_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotResultFlow {
    pub region: BooleanRegion,
    pub result: Option<String>,
    pub observations: Vec<ResultObservationRef>,
    pub completion: Coverage,
    pub unknown_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparedResultRegion {
    pub region: BooleanRegion,
    pub before_result: Option<String>,
    pub after_result: Option<String>,
    pub before_observations: Vec<ResultObservationRef>,
    pub after_observations: Vec<ResultObservationRef>,
    pub assessment: ResultAssessment,
    pub evidence: EvidenceKind,
    pub proof: Option<ResultProof>,
    pub control_changed: bool,
    pub value_dependency_changed: bool,
    pub return_structure_changed: bool,
    pub unknown_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionResultComparison {
    pub function_presence: FunctionPresence,
    pub before_entry: FunctionEntry,
    pub after_entry: FunctionEntry,
    pub before_observations: Vec<ResultObservation>,
    pub after_observations: Vec<ResultObservation>,
    pub common_domain_status: CommonDomainStatus,
    pub common_domain: Vec<BooleanRegion>,
    pub before_only_domain: Vec<BooleanRegion>,
    pub after_only_domain: Vec<BooleanRegion>,
    pub outside_both_domain: Vec<BooleanRegion>,
    pub unresolved_domain: Vec<BooleanRegion>,
    pub before_flow: Vec<SnapshotResultFlow>,
    pub after_flow: Vec<SnapshotResultFlow>,
    pub regions: Vec<ComparedResultRegion>,
    pub input_mapping_coverage: Coverage,
    pub source_alignment_coverage: Coverage,
    pub comparison_coverage: Coverage,
    pub limits_hit: BTreeSet<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Truth {
    Yes,
    No,
    Unresolved,
}

#[derive(Clone, PartialEq, Eq)]
enum ValueTerm {
    Input(u32),
    Literal(FunctionKnownValue),
    Compute(PrimitiveOperator, Vec<ValueTerm>),
}

#[derive(Clone)]
struct Selection<'a> {
    observation: Option<&'a ResultObservation>,
    unresolved: bool,
}

fn observation_ref(observation: &ResultObservation) -> ResultObservationRef {
    ResultObservationRef {
        site: observation.site.clone(),
        path_index: observation.path_index,
    }
}

fn literal_value(value: &ResultValue) -> Option<&ResultDependency> {
    match value {
        ResultValue::Undefined => None,
        ResultValue::Expression { dependency, .. } => Some(dependency),
    }
}

fn dependency_term(
    dependency: &ResultDependency,
    parameters: &[super::model::BindingId],
) -> Option<ValueTerm> {
    if dependency.unresolved {
        return None;
    }
    if let Some(Source::FunctionInput(binding)) = &dependency.origin {
        return parameters
            .iter()
            .position(|item| item == binding)
            .map(|index| ValueTerm::Input(index as u32));
    }
    if let Some(literal) = &dependency.literal {
        return Some(ValueTerm::Literal(literal.clone()));
    }
    if let Some(operator) = &dependency.operator {
        if matches!(operator, PrimitiveOperator::ValueJoin { .. }) {
            return dependency
                .inputs
                .first()
                .and_then(|input| dependency_term(input, parameters));
        }
        if !matches!(
            operator,
            PrimitiveOperator::Add
                | PrimitiveOperator::Subtract
                | PrimitiveOperator::Multiply
                | PrimitiveOperator::Divide
                | PrimitiveOperator::Remainder
                | PrimitiveOperator::UnaryPlus
                | PrimitiveOperator::UnaryMinus
                | PrimitiveOperator::LogicalNot
                | PrimitiveOperator::IsNullish
                | PrimitiveOperator::StrictEqual
                | PrimitiveOperator::StrictNotEqual
                | PrimitiveOperator::GreaterThan
                | PrimitiveOperator::LessThan
        ) {
            return None;
        }
        let inputs = dependency
            .inputs
            .iter()
            .map(|input| dependency_term(input, parameters))
            .collect::<Option<Vec<_>>>()?;
        return Some(ValueTerm::Compute(operator.clone(), inputs));
    }
    let mut inputs = dependency
        .inputs
        .iter()
        .map(|input| dependency_term(input, parameters));
    let first = inputs.next()??;
    if inputs.all(|term| term.as_ref() == Some(&first)) {
        Some(first)
    } else {
        None
    }
}

fn result_term(
    observation: &ResultObservation,
    side: &FunctionSnapshotResult,
) -> Option<ValueTerm> {
    match &observation.value {
        ResultValue::Undefined => Some(ValueTerm::Literal(FunctionKnownValue::Undefined)),
        value => dependency_term(literal_value(value)?, &side.parameters),
    }
}

fn known_number(value: &str) -> Option<f64> {
    let value: f64 = value.replace('_', "").parse().ok()?;
    (value.is_finite() && value.to_bits() != (-0.0_f64).to_bits()).then_some(value)
}

fn literal_relation(
    before: &FunctionKnownValue,
    after: &FunctionKnownValue,
) -> Option<ResultAssessment> {
    use FunctionKnownValue as V;
    for value in [before, after] {
        if let V::Number { value } = value
            && known_number(value).is_none()
        {
            return None;
        }
    }
    match (before, after) {
        (V::Number { value: a }, V::Number { value: b }) => {
            let (a, b) = (known_number(a)?, known_number(b)?);
            Some(if a.to_bits() == b.to_bits() || (a != 0.0 && a == b) {
                ResultAssessment::Equal
            } else {
                ResultAssessment::Different
            })
        }
        (V::Boolean { value: a }, V::Boolean { value: b }) => Some(if a == b {
            ResultAssessment::Equal
        } else {
            ResultAssessment::Different
        }),
        (V::String { value: a }, V::String { value: b }) => Some(if a == b {
            ResultAssessment::Equal
        } else {
            ResultAssessment::Different
        }),
        (V::Null, V::Null) | (V::Undefined, V::Undefined) => Some(ResultAssessment::Equal),
        _ => Some(ResultAssessment::Different),
    }
}

fn input_type(
    index: u32,
    before: &FunctionEntry,
    after: &FunctionEntry,
) -> Option<PrimitiveDomain> {
    let infer = |entry: &FunctionEntry| {
        entry
            .domains
            .get(&index)
            .copied()
            .or_else(|| match entry.known_values.get(&index) {
                Some(FunctionKnownValue::Boolean { .. }) => Some(PrimitiveDomain::Boolean),
                Some(FunctionKnownValue::Number { .. }) => Some(PrimitiveDomain::Number),
                Some(FunctionKnownValue::String { .. }) => Some(PrimitiveDomain::String),
                Some(FunctionKnownValue::Null | FunctionKnownValue::Undefined) => {
                    Some(PrimitiveDomain::Nullish)
                }
                None => None,
            })
    };
    let a = infer(before)?;
    (infer(after) == Some(a)).then_some(a)
}

fn term_type(
    term: &ValueTerm,
    before: &FunctionEntry,
    after: &FunctionEntry,
) -> Option<PrimitiveDomain> {
    match term {
        ValueTerm::Input(index) => input_type(*index, before, after),
        ValueTerm::Literal(FunctionKnownValue::Number { value }) => {
            known_number(value).map(|_| PrimitiveDomain::Number)
        }
        ValueTerm::Literal(FunctionKnownValue::Boolean { .. }) => Some(PrimitiveDomain::Boolean),
        ValueTerm::Literal(FunctionKnownValue::String { .. }) => Some(PrimitiveDomain::String),
        ValueTerm::Literal(FunctionKnownValue::Null | FunctionKnownValue::Undefined) => {
            Some(PrimitiveDomain::Nullish)
        }
        ValueTerm::Compute(operator, inputs) => {
            let types = inputs
                .iter()
                .map(|input| term_type(input, before, after))
                .collect::<Option<Vec<_>>>()?;
            match (operator, types.as_slice()) {
                (PrimitiveOperator::Add, [PrimitiveDomain::Number, PrimitiveDomain::Number])
                | (
                    PrimitiveOperator::Subtract
                    | PrimitiveOperator::Multiply
                    | PrimitiveOperator::Divide
                    | PrimitiveOperator::Remainder,
                    [PrimitiveDomain::Number, PrimitiveDomain::Number],
                )
                | (
                    PrimitiveOperator::UnaryPlus | PrimitiveOperator::UnaryMinus,
                    [PrimitiveDomain::Number],
                ) => Some(PrimitiveDomain::Number),
                (PrimitiveOperator::Add, [PrimitiveDomain::String, PrimitiveDomain::String]) => {
                    Some(PrimitiveDomain::String)
                }
                (PrimitiveOperator::LogicalNot | PrimitiveOperator::IsNullish, [_])
                | (PrimitiveOperator::StrictEqual | PrimitiveOperator::StrictNotEqual, [_, _])
                | (
                    PrimitiveOperator::GreaterThan | PrimitiveOperator::LessThan,
                    [PrimitiveDomain::Number, PrimitiveDomain::Number],
                ) => Some(PrimitiveDomain::Boolean),
                _ => None,
            }
        }
    }
}

fn normalized_known_term(term: &ValueTerm, entry: &FunctionEntry) -> ValueTerm {
    match term {
        ValueTerm::Input(index) => entry
            .known_values
            .get(index)
            .cloned()
            .map(ValueTerm::Literal)
            .unwrap_or_else(|| term.clone()),
        _ => term.clone(),
    }
}

fn assess(
    before: &ValueTerm,
    after: &ValueTerm,
    before_entry: &FunctionEntry,
    after_entry: &FunctionEntry,
) -> (ResultAssessment, Option<ResultProof>) {
    // Identity of a paired input needs no value evaluation or primitive type.
    // Selection, normal completion and correspondence are checked by the caller.
    if matches!((before, after), (ValueTerm::Input(a), ValueTerm::Input(b)) if a == b) {
        return (
            ResultAssessment::Equal,
            Some(ResultProof::PairedInputIdentity),
        );
    }
    let before = normalized_known_term(before, before_entry);
    let after = normalized_known_term(after, after_entry);
    if let (ValueTerm::Literal(a), ValueTerm::Literal(b)) = (&before, &after)
        && let Some(assessment) = literal_relation(a, b)
    {
        return (assessment, Some(ResultProof::PrimitiveValues));
    }
    // A resolved structural change is sufficient evidence of changed data flow.
    // Runtime type information is needed only for pure-expression equality.
    if before != after {
        return (
            ResultAssessment::Changed,
            Some(
                if matches!(
                    (&before, &after),
                    (ValueTerm::Input(_), ValueTerm::Input(_))
                ) {
                    ResultProof::ChangedSourceSelection
                } else {
                    ResultProof::ChangedComputation
                },
            ),
        );
    }
    if term_type(&before, before_entry, after_entry).is_none()
        || term_type(&after, before_entry, after_entry).is_none()
    {
        (ResultAssessment::Unknown, None)
    } else {
        (
            ResultAssessment::Equal,
            Some(ResultProof::PureExpressionIdentity),
        )
    }
}

fn bool_term(
    term: &ValueTerm,
    values: &BTreeMap<u32, bool>,
    entry: &FunctionEntry,
) -> Option<bool> {
    match term {
        ValueTerm::Input(index) => {
            values
                .get(index)
                .copied()
                .or_else(|| match entry.known_values.get(index) {
                    Some(FunctionKnownValue::Boolean { value }) => Some(*value),
                    _ => None,
                })
        }
        ValueTerm::Literal(FunctionKnownValue::Boolean { value }) => Some(*value),
        ValueTerm::Literal(FunctionKnownValue::Number { value }) => {
            known_number(value).map(|number| number != 0.0)
        }
        ValueTerm::Literal(FunctionKnownValue::String { value }) => Some(!value.is_empty()),
        ValueTerm::Literal(FunctionKnownValue::Null | FunctionKnownValue::Undefined) => Some(false),
        ValueTerm::Compute(PrimitiveOperator::LogicalNot, inputs) if inputs.len() == 1 => {
            Some(!bool_term(&inputs[0], values, entry)?)
        }
        ValueTerm::Compute(PrimitiveOperator::IsNullish, inputs) if inputs.len() == 1 => {
            match normalized_known_term(&inputs[0], entry) {
                ValueTerm::Literal(value) => Some(matches!(
                    value,
                    FunctionKnownValue::Null | FunctionKnownValue::Undefined
                )),
                term => match term_type(&term, entry, entry) {
                    Some(PrimitiveDomain::Nullish) => Some(true),
                    Some(_) => Some(false),
                    // Truthy values are non-nullish; falsy alone is insufficient.
                    None if bool_term(&term, values, entry) == Some(true) => Some(false),
                    None => None,
                },
            }
        }
        ValueTerm::Compute(PrimitiveOperator::StrictEqual, inputs)
            if inputs.len() == 2
                && inputs.iter().all(|input| {
                    term_type(input, entry, entry) == Some(PrimitiveDomain::Boolean)
                }) =>
        {
            Some(bool_term(&inputs[0], values, entry)? == bool_term(&inputs[1], values, entry)?)
        }
        ValueTerm::Compute(PrimitiveOperator::StrictNotEqual, inputs)
            if inputs.len() == 2
                && inputs.iter().all(|input| {
                    term_type(input, entry, entry) == Some(PrimitiveDomain::Boolean)
                }) =>
        {
            Some(bool_term(&inputs[0], values, entry)? != bool_term(&inputs[1], values, entry)?)
        }
        _ => None,
    }
}

fn assumption_is_boolean(expression: &AssumptionExpr, entry: &FunctionEntry) -> bool {
    match expression {
        AssumptionExpr::Input { index } => {
            entry.domains.get(index) == Some(&PrimitiveDomain::Boolean)
                || matches!(
                    entry.known_values.get(index),
                    Some(FunctionKnownValue::Boolean { .. })
                )
        }
        AssumptionExpr::Literal { value } => matches!(value, FunctionKnownValue::Boolean { .. }),
        AssumptionExpr::Unary {
            operator: AssumptionUnary::Not,
            ..
        } => true,
        AssumptionExpr::Binary {
            operator,
            left,
            right,
        } => match operator {
            AssumptionBinary::And | AssumptionBinary::Or => {
                assumption_is_boolean(left, entry) && assumption_is_boolean(right, entry)
            }
            AssumptionBinary::StrictEqual | AssumptionBinary::StrictNotEqual => true,
            _ => false,
        },
        AssumptionExpr::Conditional {
            when_true,
            when_false,
            ..
        } => assumption_is_boolean(when_true, entry) && assumption_is_boolean(when_false, entry),
        _ => false,
    }
}

fn assumption_bool(
    expression: &AssumptionExpr,
    values: &BTreeMap<u32, bool>,
    entry: &FunctionEntry,
) -> Option<bool> {
    match expression {
        AssumptionExpr::Input { index } => values.get(index).copied(),
        AssumptionExpr::Literal {
            value: FunctionKnownValue::Boolean { value },
        } => Some(*value),
        AssumptionExpr::Unary {
            operator: AssumptionUnary::Not,
            operand,
        } => Some(!assumption_bool(operand, values, entry)?),
        AssumptionExpr::Binary {
            operator: AssumptionBinary::And,
            left,
            right,
        } => {
            let a = assumption_bool(left, values, entry)?;
            if !a {
                Some(false)
            } else {
                assumption_bool(right, values, entry)
            }
        }
        AssumptionExpr::Binary {
            operator: AssumptionBinary::Or,
            left,
            right,
        } => {
            let a = assumption_bool(left, values, entry)?;
            if a {
                Some(true)
            } else {
                assumption_bool(right, values, entry)
            }
        }
        AssumptionExpr::Binary {
            operator: AssumptionBinary::StrictEqual,
            left,
            right,
        } if assumption_is_boolean(left, entry) && assumption_is_boolean(right, entry) => {
            Some(assumption_bool(left, values, entry)? == assumption_bool(right, values, entry)?)
        }
        AssumptionExpr::Binary {
            operator: AssumptionBinary::StrictNotEqual,
            left,
            right,
        } if assumption_is_boolean(left, entry) && assumption_is_boolean(right, entry) => {
            Some(assumption_bool(left, values, entry)? != assumption_bool(right, values, entry)?)
        }
        AssumptionExpr::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            if assumption_bool(condition, values, entry)? {
                assumption_bool(when_true, values, entry)
            } else {
                assumption_bool(when_false, values, entry)
            }
        }
        _ => None,
    }
}

fn entry_truth(entry: &FunctionEntry, values: &BTreeMap<u32, bool>) -> Truth {
    let mut unknown = entry.known_values.values().any(|value| match value {
        FunctionKnownValue::Number { value } => known_number(value).is_none(),
        _ => false,
    });
    for (index, known) in &entry.known_values {
        if let Some(actual) = values.get(index) {
            let truthiness = match known {
                FunctionKnownValue::Boolean { value } => Some(*value),
                FunctionKnownValue::Number { value } => known_number(value).map(|n| n != 0.0),
                FunctionKnownValue::String { value } => Some(!value.is_empty()),
                FunctionKnownValue::Null | FunctionKnownValue::Undefined => Some(false),
            };
            match truthiness {
                Some(value) if value == *actual => {}
                Some(_) => return Truth::No,
                None => unknown = true,
            }
        }
    }
    for assumption in &entry.assumptions {
        match assumption_bool(assumption, values, entry) {
            Some(false) => return Truth::No,
            Some(true) => {}
            None => unknown = true,
        }
    }
    if unknown {
        Truth::Unresolved
    } else {
        Truth::Yes
    }
}

fn value_domain(value: &FunctionKnownValue) -> PrimitiveDomain {
    match value {
        FunctionKnownValue::Boolean { .. } => PrimitiveDomain::Boolean,
        FunctionKnownValue::Number { .. } => PrimitiveDomain::Number,
        FunctionKnownValue::String { .. } => PrimitiveDomain::String,
        FunctionKnownValue::Null | FunctionKnownValue::Undefined => PrimitiveDomain::Nullish,
    }
}

fn entries_disjoint(before: &FunctionEntry, after: &FunctionEntry) -> bool {
    for (index, domain) in &before.domains {
        if after
            .domains
            .get(index)
            .is_some_and(|other| other != domain)
            || after
                .known_values
                .get(index)
                .is_some_and(|value| value_domain(value) != *domain)
        {
            return true;
        }
    }
    for (index, value) in &before.known_values {
        if after
            .domains
            .get(index)
            .is_some_and(|domain| *domain != value_domain(value))
            || after.known_values.get(index).is_some_and(|other| {
                literal_relation(value, other) == Some(ResultAssessment::Different)
            })
        {
            return true;
        }
    }
    false
}

fn select_observation<'a>(
    side: &'a FunctionSnapshotResult,
    values: &BTreeMap<u32, bool>,
) -> Selection<'a> {
    let mut selected = Vec::new();
    let mut unresolved = false;
    for observation in &side.observations {
        let mut rejected = false;
        let mut uncertain = false;
        for guard in &observation.guards {
            let value = dependency_term(&guard.condition, &side.parameters)
                .and_then(|term| bool_term(&term, values, &side.entry));
            match value {
                Some(value) if value != guard.outcome => {
                    rejected = true;
                    break;
                }
                Some(_) => {}
                None => uncertain = true,
            }
        }
        if !rejected {
            if uncertain {
                unresolved = true;
            } else {
                selected.push(observation);
            }
        }
    }
    let observation = (selected.len() == 1).then(|| selected[0]);
    if let Some(observation) = observation {
        unresolved |= observation.dependency_coverage != Coverage::Complete
            || observation.unknown_completion_before_return;
    }
    Selection {
        observation,
        unresolved: unresolved || selected.len() != 1,
    }
}

fn value_text(observation: &ResultObservation) -> String {
    match &observation.value {
        ResultValue::Undefined => "undefined".into(),
        ResultValue::Expression { text, .. } => text.clone(),
    }
}

fn dependency_inputs(
    dependency: &ResultDependency,
    parameters: &[super::model::BindingId],
    out: &mut BTreeSet<u32>,
) {
    if let Some(Source::FunctionInput(binding)) = &dependency.origin
        && let Some(index) = parameters.iter().position(|parameter| parameter == binding)
    {
        out.insert(index as u32);
    }
    for input in &dependency.inputs {
        dependency_inputs(input, parameters, out);
    }
}

fn result_inputs(observation: &ResultObservation, side: &FunctionSnapshotResult) -> BTreeSet<u32> {
    let mut inputs = BTreeSet::new();
    if let Some(dependency) = literal_value(&observation.value) {
        dependency_inputs(dependency, &side.parameters, &mut inputs);
    }
    inputs
}

fn dependency_structure(
    dependency: &ResultDependency,
    parameters: &[super::model::BindingId],
) -> String {
    let source = if let Some(Source::FunctionInput(binding)) = &dependency.origin {
        format!(
            "input:{}",
            parameters
                .iter()
                .position(|parameter| parameter == binding)
                .map_or("?".into(), |index| index.to_string())
        )
    } else if let Some(literal) = &dependency.literal {
        format!("literal:{literal:?}")
    } else {
        dependency.operation.clone()
    };
    let children: Vec<_> = dependency
        .inputs
        .iter()
        .map(|input| dependency_structure(input, parameters))
        .collect();
    format!("{source}({})", children.join(","))
}

fn return_structure(observation: &ResultObservation, side: &FunctionSnapshotResult) -> String {
    match &observation.value {
        ResultValue::Undefined => "undefined".into(),
        ResultValue::Expression { dependency, .. } => {
            dependency_structure(dependency, &side.parameters)
        }
    }
}

fn guard_signature(
    observation: &ResultObservation,
    side: &FunctionSnapshotResult,
) -> Vec<(bool, Option<String>)> {
    observation
        .guards
        .iter()
        .map(|guard| {
            let term = dependency_term(&guard.condition, &side.parameters);
            (guard.outcome, term.map(|term| format!("{term:?}")))
        })
        .collect()
}

impl std::fmt::Debug for ValueTerm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input(index) => write!(f, "input({index})"),
            Self::Literal(value) => write!(f, "{value:?}"),
            Self::Compute(operator, inputs) => write!(f, "{operator:?}({inputs:?})"),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct AtomSignature {
    assessment: ResultAssessment,
    before_term: Option<ValueTerm>,
    after_term: Option<ValueTerm>,
    unknown_observation_pair: Option<(ResultObservationRef, ResultObservationRef)>,
    proof: Option<ResultProof>,
    diagnostic: Option<DiagnosticCode>,
    before_result: Option<String>,
    after_result: Option<String>,
    control_changed: bool,
    value_dependency_changed: bool,
    return_structure_changed: bool,
    reason: Option<String>,
}

struct Atom {
    bits: Vec<bool>,
    signature: AtomSignature,
    before_ref: Option<ResultObservationRef>,
    after_ref: Option<ResultObservationRef>,
}

fn merge_cubes(mut cubes: Vec<Vec<Option<bool>>>) -> Vec<Vec<Option<bool>>> {
    cubes.sort();
    cubes.dedup();
    loop {
        let mut next = Vec::new();
        let mut used = vec![false; cubes.len()];
        for a in 0..cubes.len() {
            if used[a] {
                continue;
            }
            for b in (a + 1)..cubes.len() {
                if used[b] {
                    continue;
                }
                let differences: Vec<_> = cubes[a]
                    .iter()
                    .zip(&cubes[b])
                    .enumerate()
                    .filter_map(|(index, (x, y))| (x != y).then_some(index))
                    .collect();
                if differences.len() == 1 {
                    let index = differences[0];
                    if cubes[a][index].is_some() && cubes[b][index].is_some() {
                        let mut merged = cubes[a].clone();
                        merged[index] = None;
                        next.push(merged);
                        used[a] = true;
                        used[b] = true;
                        break;
                    }
                }
            }
        }
        next.extend(
            cubes
                .iter()
                .zip(used)
                .filter_map(|(cube, used)| (!used).then_some(cube.clone())),
        );
        next.sort();
        next.dedup();
        if next == cubes {
            return next;
        }
        cubes = next;
    }
}

fn region(slots: &[u32], cube: &[Option<bool>]) -> BooleanRegion {
    BooleanRegion {
        values: slots
            .iter()
            .zip(cube)
            .filter_map(|(slot, value)| value.map(|value| (*slot, value)))
            .collect(),
    }
}

fn grouped_domains(slots: &[u32], assignments: &[Vec<bool>]) -> Vec<BooleanRegion> {
    merge_cubes(
        assignments
            .iter()
            .map(|bits| bits.iter().copied().map(Some).collect())
            .collect(),
    )
    .iter()
    .map(|cube| region(slots, cube))
    .collect()
}

fn collect_slots(analysis: &FunctionResultAnalysis) -> Vec<u32> {
    fn term_slots(term: &ValueTerm, slots: &mut BTreeSet<u32>) {
        match term {
            ValueTerm::Input(index) => {
                slots.insert(*index);
            }
            ValueTerm::Compute(_, inputs) => {
                for input in inputs {
                    term_slots(input, slots);
                }
            }
            ValueTerm::Literal(_) => {}
        }
    }
    fn assumption_slots(expr: &AssumptionExpr, slots: &mut BTreeSet<u32>) {
        match expr {
            AssumptionExpr::Input { index } => {
                slots.insert(*index);
            }
            AssumptionExpr::Unary { operand, .. } => assumption_slots(operand, slots),
            AssumptionExpr::Binary { left, right, .. } => {
                assumption_slots(left, slots);
                assumption_slots(right, slots);
            }
            AssumptionExpr::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                assumption_slots(condition, slots);
                assumption_slots(when_true, slots);
                assumption_slots(when_false, slots);
            }
            AssumptionExpr::Unsupported { inputs, .. } => slots.extend(inputs.iter().copied()),
            AssumptionExpr::Literal { .. } => {}
        }
    }
    let mut slots = BTreeSet::new();
    for entry in [&analysis.query.before_entry, &analysis.query.after_entry] {
        for (index, domain) in &entry.domains {
            if *domain == PrimitiveDomain::Boolean {
                slots.insert(*index);
            }
        }
        for (index, known) in &entry.known_values {
            if matches!(known, FunctionKnownValue::Boolean { .. }) {
                slots.insert(*index);
            }
        }
        for assumption in &entry.assumptions {
            assumption_slots(assumption, &mut slots);
        }
    }
    for side in [&analysis.before, &analysis.after].into_iter().flatten() {
        for observation in &side.observations {
            for guard in &observation.guards {
                if let Some(term) = dependency_term(&guard.condition, &side.parameters) {
                    term_slots(&term, &mut slots);
                }
            }
        }
    }
    slots.into_iter().collect()
}

type FlowAtom = (
    Vec<bool>,
    Option<String>,
    Option<ResultObservationRef>,
    Coverage,
    Option<String>,
);

fn group_flows(slots: &[u32], atoms: &[FlowAtom]) -> Vec<SnapshotResultFlow> {
    let mut pending = atoms.to_vec();
    let mut output = Vec::new();
    while let Some(seed) = pending.pop() {
        let mut group = vec![seed];
        let mut index = 0;
        while index < pending.len() {
            if pending[index].1 == group[0].1
                && pending[index].3 == group[0].3
                && pending[index].4 == group[0].4
            {
                group.push(pending.remove(index));
            } else {
                index += 1;
            }
        }
        let cubes = merge_cubes(
            group
                .iter()
                .map(|atom| atom.0.iter().copied().map(Some).collect())
                .collect(),
        );
        for cube in cubes {
            let mut observations = Vec::new();
            for atom in group.iter().filter(|atom| {
                cube.iter()
                    .zip(&atom.0)
                    .all(|(part, bit)| part.is_none_or(|part| part == *bit))
            }) {
                if let Some(reference) = &atom.2
                    && !observations.contains(reference)
                {
                    observations.push(reference.clone());
                }
            }
            observations.sort();
            output.push(SnapshotResultFlow {
                region: region(slots, &cube),
                result: group[0].1.clone(),
                observations,
                completion: group[0].3,
                unknown_reason: group[0].4.clone(),
            });
        }
    }
    output.sort_by(|a, b| a.region.values.cmp(&b.region.values));
    output
}

/// Refine already established side flows by their selected return observation.
/// This changes presentation granularity, not the domain or value assessment.
pub(crate) fn split_result_flows(
    analysis: &FunctionResultAnalysis,
    side: &Option<FunctionSnapshotResult>,
    flows: &[SnapshotResultFlow],
) -> Vec<SnapshotResultFlow> {
    let Some(side) = side else {
        return flows.to_vec();
    };
    let slots = collect_slots(analysis);
    if slots.len() > 12 {
        return flows.to_vec();
    }
    let mut output = Vec::new();
    for flow in flows {
        if flow.observations.len() <= 1 || flow.completion != Coverage::Complete {
            output.push(flow.clone());
            continue;
        }
        let mut groups: BTreeMap<ResultObservationRef, Vec<Vec<bool>>> = BTreeMap::new();
        let mut unresolved = false;
        for mask in 0..(1_usize << slots.len()) {
            let bits: Vec<_> = (0..slots.len())
                .map(|index| mask & (1 << index) != 0)
                .collect();
            let values: BTreeMap<_, _> = slots.iter().copied().zip(bits.iter().copied()).collect();
            if !flow
                .region
                .values
                .iter()
                .all(|(slot, value)| values.get(slot) == Some(value))
            {
                continue;
            }
            let selection = select_observation(side, &values);
            if let Some(item) = selection.observation.filter(|_| !selection.unresolved) {
                let reference = observation_ref(item);
                if flow.observations.contains(&reference) {
                    groups.entry(reference).or_default().push(bits);
                    continue;
                }
            }
            unresolved = true;
            break;
        }
        if unresolved {
            output.push(flow.clone());
            continue;
        }
        for (reference, bits) in groups {
            for region in grouped_domains(&slots, &bits) {
                output.push(SnapshotResultFlow {
                    region,
                    observations: vec![reference.clone()],
                    ..flow.clone()
                });
            }
        }
    }
    output.sort_by(|a, b| {
        (&a.region.values, &a.observations).cmp(&(&b.region.values, &b.observations))
    });
    output
}

/// Compare supported normal results under the paired-input domain.
/// The two side flows remain available even when that domain has no intersection.
pub fn compare_function_results(analysis: &FunctionResultAnalysis) -> FunctionResultComparison {
    compare_function_results_with_limit(
        analysis,
        analysis.query.limits.processed_path_edges.min(4096) as usize,
    )
}

/// A separate comparison budget permits partial established regions to survive.
pub fn compare_function_results_with_limit(
    analysis: &FunctionResultAnalysis,
    max_assignments: usize,
) -> FunctionResultComparison {
    let before_entry = analysis.query.before_entry.clone();
    let after_entry = analysis.query.after_entry.clone();
    let before_observations = analysis
        .before
        .as_ref()
        .map_or_else(Vec::new, |side| side.observations.clone());
    let after_observations = analysis
        .after
        .as_ref()
        .map_or_else(Vec::new, |side| side.observations.clone());
    let disjoint_entries = entries_disjoint(&before_entry, &after_entry);
    let function_presence = match (
        &analysis.before,
        &analysis.after,
        &analysis.counterpart_status,
    ) {
        (Some(_), Some(_), FunctionCounterpart::Matched) => FunctionPresence::Both,
        (Some(_), None, FunctionCounterpart::ConfirmedAbsent) => FunctionPresence::BeforeOnly,
        (None, Some(_), FunctionCounterpart::ConfirmedAbsent) => FunctionPresence::AfterOnly,
        _ => FunctionPresence::Unresolved,
    };
    let source_alignment_coverage = match (&analysis.before, &analysis.after) {
        (Some(before), Some(after))
            if before.function.declaration == after.function.declaration
                && !analysis.query.diff.changes.iter().any(|change| {
                    change.before_path.as_deref() == Some(&before.function.declaration.path)
                        || change.after_path.as_deref() == Some(&after.function.declaration.path)
                }) =>
        {
            Coverage::Complete
        }
        _ => Coverage::Partial,
    };
    let slots = collect_slots(analysis);
    let mut limits_hit = BTreeSet::new();
    if slots.len() > 12 {
        limits_hit.insert("boolean_regions".into());
        return FunctionResultComparison {
            function_presence,
            before_entry,
            after_entry,
            before_observations,
            after_observations,
            common_domain_status: CommonDomainStatus::Unresolved,
            common_domain: vec![],
            before_only_domain: vec![],
            after_only_domain: vec![],
            outside_both_domain: vec![],
            unresolved_domain: vec![],
            before_flow: vec![],
            after_flow: vec![],
            regions: vec![],
            input_mapping_coverage: analysis.input_mapping_coverage,
            source_alignment_coverage,
            comparison_coverage: Coverage::Partial,
            limits_hit,
        };
    }
    let mut common = Vec::new();
    let mut before_only = Vec::new();
    let mut after_only = Vec::new();
    let mut outside = Vec::new();
    let mut unresolved = Vec::new();
    let mut before_flow_atoms = Vec::new();
    let mut after_flow_atoms = Vec::new();
    let mut atoms = Vec::new();
    let total_assignments = 1_usize << slots.len();
    let examined_assignments = total_assignments.min(max_assignments);
    if examined_assignments < total_assignments {
        limits_hit.insert("comparison_assignments".into());
    }
    for mask in 0..total_assignments {
        let bits: Vec<bool> = (0..slots.len())
            .map(|index| mask & (1 << index) != 0)
            .collect();
        if mask >= examined_assignments {
            unresolved.push(bits);
            continue;
        }
        let values: BTreeMap<u32, bool> = slots.iter().copied().zip(bits.iter().copied()).collect();
        let before_truth = entry_truth(&analysis.query.before_entry, &values);
        let after_truth = entry_truth(&analysis.query.after_entry, &values);
        match (before_truth, after_truth) {
            (Truth::Yes, Truth::Yes) if disjoint_entries => {
                before_only.push(bits.clone());
                after_only.push(bits.clone());
            }
            (Truth::Yes, Truth::Yes) => common.push(bits.clone()),
            (Truth::Yes, Truth::No) => before_only.push(bits.clone()),
            (Truth::No, Truth::Yes) => after_only.push(bits.clone()),
            (Truth::No, Truth::No) => outside.push(bits.clone()),
            _ => unresolved.push(bits.clone()),
        }
        let before_selection = if before_truth == Truth::Yes {
            analysis
                .before
                .as_ref()
                .map(|side| select_observation(side, &values))
        } else {
            None
        };
        let after_selection = if after_truth == Truth::Yes {
            analysis
                .after
                .as_ref()
                .map(|side| select_observation(side, &values))
        } else {
            None
        };
        if let Some(selection) = &before_selection {
            before_flow_atoms.push((
                bits.clone(),
                selection.observation.map(value_text),
                selection.observation.map(observation_ref),
                if selection.unresolved {
                    Coverage::Partial
                } else {
                    Coverage::Complete
                },
                selection
                    .unresolved
                    .then(|| "normal completion or result selection is unresolved".into()),
            ));
        }
        if let Some(selection) = &after_selection {
            after_flow_atoms.push((
                bits.clone(),
                selection.observation.map(value_text),
                selection.observation.map(observation_ref),
                if selection.unresolved {
                    Coverage::Partial
                } else {
                    Coverage::Complete
                },
                selection
                    .unresolved
                    .then(|| "normal completion or result selection is unresolved".into()),
            ));
        }
        if disjoint_entries || before_truth != Truth::Yes || after_truth != Truth::Yes {
            continue;
        }
        let (before_side, after_side) = match (&analysis.before, &analysis.after) {
            (Some(before), Some(after))
                if analysis.counterpart_status == FunctionCounterpart::Matched =>
            {
                (before, after)
            }
            _ => continue,
        };
        let before_selection = before_selection.expect("selected side exists");
        let after_selection = after_selection.expect("selected side exists");
        let before_observation = before_selection.observation;
        let after_observation = after_selection.observation;
        let before_term =
            before_observation.and_then(|observation| result_term(observation, before_side));
        let after_term =
            after_observation.and_then(|observation| result_term(observation, after_side));
        let (assessment, proof, diagnostic, reason) = if analysis.input_mapping_coverage
            != Coverage::Complete
        {
            (
                ResultAssessment::Unknown,
                None,
                Some(DiagnosticCode::AmbiguousMatch),
                Some("input correspondence is unresolved".into()),
            )
        } else if before_selection.unresolved || after_selection.unresolved {
            let unresolved_call = before_observation
                .into_iter()
                .chain(after_observation)
                .any(|observation| observation.unknown_completion_before_return);
            (
                ResultAssessment::Unknown,
                None,
                Some(if unresolved_call {
                    DiagnosticCode::UnresolvedCall
                } else {
                    DiagnosticCode::UnprovenPathFeasibility
                }),
                Some(if unresolved_call {
                    "an unresolved call may prevent normal completion".into()
                } else {
                    "normal completion or result selection is unresolved".into()
                }),
            )
        } else if before_observation.is_some() && after_observation.is_some() {
            match (&before_term, &after_term) {
                (Some(a), Some(b)) => {
                    let (assessment, proof) = assess(a, b, &before_side.entry, &after_side.entry);
                    (
                        assessment,
                        proof,
                        (assessment == ResultAssessment::Unknown)
                            .then_some(DiagnosticCode::TypeResolutionUnavailable),
                        (assessment == ResultAssessment::Unknown)
                            .then(|| "result value theory is insufficient".into()),
                    )
                }
                _ => (
                    ResultAssessment::Unknown,
                    None,
                    Some(DiagnosticCode::MissingDependency),
                    Some("returned computation is unresolved".into()),
                ),
            }
        } else {
            (
                ResultAssessment::Unknown,
                None,
                Some(DiagnosticCode::MissingDependency),
                Some("normal return is unresolved".into()),
            )
        };
        let control_changed =
            before_observation
                .zip(after_observation)
                .is_some_and(|(before, after)| {
                    guard_signature(before, before_side) != guard_signature(after, after_side)
                });
        let value_dependency_changed =
            before_observation
                .zip(after_observation)
                .is_some_and(|(before, after)| match (&before_term, &after_term) {
                    (Some(a), Some(b)) => a != b,
                    _ => result_inputs(before, before_side) != result_inputs(after, after_side),
                });
        let return_structure_changed =
            before_observation
                .zip(after_observation)
                .is_some_and(|(before, after)| {
                    return_structure(before, before_side) != return_structure(after, after_side)
                });
        atoms.push(Atom {
            bits,
            signature: AtomSignature {
                assessment,
                before_term,
                after_term,
                unknown_observation_pair: if assessment == ResultAssessment::Unknown {
                    before_observation
                        .zip(after_observation)
                        .map(|(before, after)| (observation_ref(before), observation_ref(after)))
                } else {
                    None
                },
                proof,
                diagnostic,
                before_result: before_observation.map(value_text),
                after_result: after_observation.map(value_text),
                control_changed,
                value_dependency_changed,
                return_structure_changed,
                reason,
            },
            before_ref: before_observation.map(observation_ref),
            after_ref: after_observation.map(observation_ref),
        });
    }
    let mut regions = Vec::new();
    while let Some(seed) = atoms.pop() {
        let mut group = vec![seed];
        let mut index = 0;
        while index < atoms.len() {
            if atoms[index].signature == group[0].signature {
                group.push(atoms.remove(index));
            } else {
                index += 1;
            }
        }
        let cubes = merge_cubes(
            group
                .iter()
                .map(|atom| atom.bits.iter().copied().map(Some).collect())
                .collect(),
        );
        for cube in cubes {
            let matching = |bits: &[bool]| {
                cube.iter()
                    .zip(bits)
                    .all(|(part, bit)| part.is_none_or(|part| part == *bit))
            };
            let mut before_refs = Vec::new();
            let mut after_refs = Vec::new();
            for atom in group.iter().filter(|atom| matching(&atom.bits)) {
                if let Some(reference) = &atom.before_ref
                    && !before_refs.contains(reference)
                {
                    before_refs.push(reference.clone());
                }
                if let Some(reference) = &atom.after_ref
                    && !after_refs.contains(reference)
                {
                    after_refs.push(reference.clone());
                }
            }
            let signature = &group[0].signature;
            before_refs.sort();
            after_refs.sort();
            regions.push(ComparedResultRegion {
                region: region(&slots, &cube),
                before_result: signature.before_result.clone(),
                after_result: signature.after_result.clone(),
                before_observations: before_refs,
                after_observations: after_refs,
                assessment: signature.assessment,
                evidence: signature
                    .diagnostic
                    .clone()
                    .map_or(EvidenceKind::Supported, |diagnostic| {
                        EvidenceKind::Unresolved { diagnostic }
                    }),
                proof: signature.proof,
                control_changed: signature.control_changed,
                value_dependency_changed: signature.value_dependency_changed,
                return_structure_changed: signature.return_structure_changed,
                unknown_reason: signature.reason.clone(),
            });
        }
    }
    regions.sort_by(|a, b| a.region.values.cmp(&b.region.values));
    let comparison_coverage = if function_presence != FunctionPresence::Both {
        Coverage::Unsupported
    } else if unresolved.is_empty()
        && regions
            .iter()
            .all(|region| region.assessment != ResultAssessment::Unknown)
        && (common.is_empty()
            || (analysis
                .before
                .as_ref()
                .is_some_and(|side| side.coverage == Coverage::Complete)
                && analysis
                    .after
                    .as_ref()
                    .is_some_and(|side| side.coverage == Coverage::Complete)))
    {
        Coverage::Complete
    } else {
        Coverage::Partial
    };
    FunctionResultComparison {
        function_presence,
        before_entry,
        after_entry,
        before_observations,
        after_observations,
        common_domain_status: if !common.is_empty() {
            CommonDomainStatus::Present
        } else if unresolved.is_empty() {
            CommonDomainStatus::Empty
        } else {
            CommonDomainStatus::Unresolved
        },
        common_domain: grouped_domains(&slots, &common),
        before_only_domain: grouped_domains(&slots, &before_only),
        after_only_domain: grouped_domains(&slots, &after_only),
        outside_both_domain: grouped_domains(&slots, &outside),
        unresolved_domain: grouped_domains(&slots, &unresolved),
        before_flow: group_flows(&slots, &before_flow_atoms),
        after_flow: group_flows(&slots, &after_flow_atoms),
        regions,
        input_mapping_coverage: analysis.input_mapping_coverage,
        source_alignment_coverage,
        comparison_coverage,
        limits_hit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ifds::adapters::typescript::index_functions;
    use crate::ifds::model::*;
    use crate::ifds::snapshots::{AnalysisEnvironment, InMemorySnapshot, InMemorySnapshotProvider};

    fn entry(domains: &[(u32, PrimitiveDomain)]) -> FunctionEntry {
        FunctionEntry {
            domains: domains.iter().copied().collect(),
            ..FunctionEntry::default()
        }
    }

    fn analyze_sources(
        before_source: &str,
        after_source: &str,
        before_entry: FunctionEntry,
        after_entry: FunctionEntry,
    ) -> FunctionResultAnalysis {
        let path = "src/fixture.ts";
        let before_id = SnapshotId {
            side: SnapshotSide::Before,
            revision: "before".into(),
            content_id: "tree-before".into(),
        };
        let after_id = SnapshotId {
            side: SnapshotSide::After,
            revision: "after".into(),
            content_id: "tree-after".into(),
        };
        let before_handle = SnapshotHandle {
            id: before_id.clone(),
            repository_id: "fixture".into(),
        };
        let after_handle = SnapshotHandle {
            id: after_id.clone(),
            repository_id: "fixture".into(),
        };
        let provider = InMemorySnapshotProvider::new([
            InMemorySnapshot {
                handle: before_handle.clone(),
                files: BTreeMap::from([(
                    path.into(),
                    ("blob-before".into(), before_source.as_bytes().to_vec()),
                )]),
            },
            InMemorySnapshot {
                handle: after_handle.clone(),
                files: BTreeMap::from([(
                    path.into(),
                    ("blob-after".into(), after_source.as_bytes().to_vec()),
                )]),
            },
        ]);
        let selected = index_functions(path, before_source).unwrap().remove(0);
        let capabilities = CapabilitySet {
            stage: 1,
            capabilities: BTreeSet::new(),
            version: "test".into(),
        };
        let query = FunctionResultQuery {
            before: before_handle,
            after: after_handle,
            diff: RepositoryDiff {
                before_content_id: "tree-before".into(),
                after_content_id: "tree-after".into(),
                changes: BTreeSet::from([FileChange {
                    kind: FileChangeKind::Modified,
                    before_path: Some(path.into()),
                    after_path: Some(path.into()),
                    before_content_id: Some("blob-before".into()),
                    after_content_id: Some("blob-after".into()),
                }]),
            },
            selected_function: FunctionSelector {
                snapshot: before_id,
                declaration: selected.span,
                expected_name: selected.name,
                expected_enclosing_symbol: selected.owner,
            },
            counterpart: None,
            before_entry,
            after_entry,
            capabilities: capabilities.clone(),
            summaries: BTreeSet::new(),
            limits: AnalysisLimits {
                time_ms: 30_000,
                memory_bytes: 256_000_000,
                processed_path_edges: 100_000,
                witnesses_per_relation: 10,
                output_nodes: 10_000,
            },
        };
        let environment = AnalysisEnvironment {
            capabilities,
            summaries: BTreeSet::new(),
        };
        analyze_function_result(query, &provider, &environment, &environment).unwrap()
    }

    fn compare_sources(
        before_source: &str,
        after_source: &str,
        before_entry: FunctionEntry,
        after_entry: FunctionEntry,
    ) -> FunctionResultComparison {
        compare_function_results(&analyze_sources(
            before_source,
            after_source,
            before_entry,
            after_entry,
        ))
    }

    #[test]
    fn ifds_fr002_untyped_paired_input_identity() {
        for (before, after) in [
            (
                "function result(flag) { return flag; }",
                "function result(renamed) { const copy = renamed; return copy; }",
            ),
            (
                "function result(flag, other) { const saved = flag; flag = other; return saved; }",
                "function result(flag, other) { return flag; }",
            ),
        ] {
            let result = compare_sources(
                before,
                after,
                FunctionEntry::default(),
                FunctionEntry::default(),
            );
            assert_eq!(result.regions.len(), 1, "{result:#?}");
            assert_eq!(result.regions[0].assessment, ResultAssessment::Equal);
            assert_eq!(
                result.regions[0].proof,
                Some(ResultProof::PairedInputIdentity)
            );
            assert_eq!(result.comparison_coverage, Coverage::Complete);
        }

        for (after, assessment, proof, coverage) in [
            (
                "function result(p, q) { return q; }",
                ResultAssessment::Changed,
                Some(ResultProof::ChangedSourceSelection),
                Coverage::Complete,
            ),
            (
                "function result(p, q) { p = q; return p; }",
                ResultAssessment::Changed,
                Some(ResultProof::ChangedSourceSelection),
                Coverage::Complete,
            ),
            (
                "function result(p, q) { return p + 1; }",
                ResultAssessment::Changed,
                Some(ResultProof::ChangedComputation),
                Coverage::Complete,
            ),
            (
                "function result(p, q) { mystery(); return p; }",
                ResultAssessment::Unknown,
                None,
                Coverage::Partial,
            ),
        ] {
            let result = compare_sources(
                "function result(p, q) { return p; }",
                after,
                FunctionEntry::default(),
                FunctionEntry::default(),
            );
            assert_eq!(
                result.regions[0].assessment, assessment,
                "{after}: {result:#?}"
            );
            assert_eq!(result.regions[0].proof, proof);
            assert_eq!(result.comparison_coverage, coverage);
        }
        let mut analysis = analyze_sources(
            "function result(p) { return p; }",
            "function result(p) { return p; }",
            FunctionEntry::default(),
            FunctionEntry::default(),
        );
        analysis.input_mapping_coverage = Coverage::Partial;
        assert_eq!(
            compare_function_results(&analysis).regions[0].assessment,
            ResultAssessment::Unknown
        );
    }

    #[test]
    fn ifds_fr002_short_circuit_input_identity() {
        for (operator, skip_when) in [("&&", false), ("||", true)] {
            for prefix in ["", "const saved = flag; "] {
                let left = if prefix.is_empty() { "flag" } else { "saved" };
                let before = format!(
                    "function result(flag) {{ {prefix}return {left} {operator} \"old\"; }}"
                );
                let after = format!(
                    "function result(flag) {{ {prefix}return {left} {operator} \"new\"; }}"
                );
                let result = compare_sources(
                    &before,
                    &after,
                    FunctionEntry::default(),
                    FunctionEntry::default(),
                );
                assert_eq!(result.regions.len(), 2, "{result:#?}");
                for region in &result.regions {
                    let skips = region.region.values == BTreeMap::from([(0, skip_when)]);
                    assert_eq!(
                        region.assessment,
                        if skips {
                            ResultAssessment::Equal
                        } else {
                            ResultAssessment::Different
                        }
                    );
                    if skips {
                        assert_eq!(region.proof, Some(ResultProof::PairedInputIdentity));
                    }
                }
                assert_eq!(result.comparison_coverage, Coverage::Complete);
            }
        }
        let result = compare_sources(
            "function result(a, b) { return a && (b || \"old\"); }",
            "function result(a, b) { return a && (b || \"new\"); }",
            FunctionEntry::default(),
            FunctionEntry::default(),
        );
        for (condition, expected) in [
            (BTreeMap::from([(0, false)]), ResultAssessment::Equal),
            (
                BTreeMap::from([(0, true), (1, true)]),
                ResultAssessment::Equal,
            ),
            (
                BTreeMap::from([(0, true), (1, false)]),
                ResultAssessment::Different,
            ),
        ] {
            assert!(
                result.regions.iter().any(
                    |region| region.region.values == condition && region.assessment == expected
                ),
                "{result:#?}"
            );
        }
        assert_eq!(result.comparison_coverage, Coverage::Complete);
    }

    #[test]
    fn ifds_fr002_nullish_operand_selection() {
        let before = "function result(flag) { const saved = flag; return saved ?? \"old\"; }";
        let after = "function result(flag) { const saved = flag; return saved ?? \"new\"; }";
        for domain in [
            PrimitiveDomain::Boolean,
            PrimitiveDomain::Number,
            PrimitiveDomain::String,
            PrimitiveDomain::Nullish,
        ] {
            let scope = entry(&[(0, domain)]);
            let result = compare_sources(before, after, scope.clone(), scope);
            let expected = if domain == PrimitiveDomain::Nullish {
                ResultAssessment::Different
            } else {
                ResultAssessment::Equal
            };
            assert!(!result.regions.is_empty());
            assert!(
                result
                    .regions
                    .iter()
                    .all(|region| region.assessment == expected),
                "{domain:?}: {result:#?}"
            );
            assert_eq!(result.comparison_coverage, Coverage::Complete);
        }
        for value in [
            FunctionKnownValue::Boolean { value: false },
            FunctionKnownValue::Number { value: "0".into() },
            FunctionKnownValue::String {
                value: String::new(),
            },
            FunctionKnownValue::Null,
            FunctionKnownValue::Undefined,
        ] {
            let expected = if matches!(
                value,
                FunctionKnownValue::Null | FunctionKnownValue::Undefined
            ) {
                ResultAssessment::Different
            } else {
                ResultAssessment::Equal
            };
            let mut scope = FunctionEntry::default();
            scope.known_values.insert(0, value.clone());
            let result = compare_sources(before, after, scope.clone(), scope);
            assert!(!result.regions.is_empty());
            assert!(
                result
                    .regions
                    .iter()
                    .all(|region| region.assessment == expected),
                "{value:?}: {result:#?}"
            );
            assert_eq!(result.comparison_coverage, Coverage::Complete);
        }
        let result = compare_sources(
            before,
            after,
            FunctionEntry::default(),
            FunctionEntry::default(),
        );
        assert!(
            result
                .regions
                .iter()
                .any(|region| region.region.values == BTreeMap::from([(0, true)])
                    && region.assessment == ResultAssessment::Equal),
            "{result:#?}"
        );
        assert!(result.regions.iter().any(|region| region.region.values
            == BTreeMap::from([(0, false)])
            && region.assessment == ResultAssessment::Unknown));
        assert_eq!(result.comparison_coverage, Coverage::Partial);
    }

    #[test]
    fn ifds_fr002_skipped_call_input_identity() {
        for (operator, skip_when) in [("&&", false), ("||", true)] {
            let before = format!("function result(flag) {{ return flag {operator} \"old\"; }}");
            let after = format!("function result(flag) {{ return flag {operator} mystery(); }}");
            let result = compare_sources(
                &before,
                &after,
                FunctionEntry::default(),
                FunctionEntry::default(),
            );
            assert_eq!(result.regions.len(), 2, "{result:#?}");
            for region in &result.regions {
                let skipped = region.region.values == BTreeMap::from([(0, skip_when)]);
                assert_eq!(
                    region.assessment,
                    if skipped {
                        ResultAssessment::Equal
                    } else {
                        ResultAssessment::Unknown
                    },
                    "{region:#?}"
                );
                if skipped {
                    assert_eq!(region.proof, Some(ResultProof::PairedInputIdentity));
                } else {
                    assert!(
                        region
                            .unknown_reason
                            .as_deref()
                            .unwrap()
                            .contains("unresolved call")
                    );
                }
            }
            assert_eq!(result.comparison_coverage, Coverage::Partial);
        }
    }

    #[test]
    fn ifds_fr002_guard_added() {
        let before = "function result(enabled: boolean, ready: boolean) { if (enabled) return \"ok\"; return \"skip\"; }";
        let after = "function result(enabled: boolean, ready: boolean) { if (enabled && ready) return \"ok\"; return \"skip\"; }";
        let entry = FunctionEntry::default();
        let result = compare_sources(before, after, entry.clone(), entry);
        assert_eq!(result.regions.len(), 3, "{result:#?}");
        assert_eq!(
            result
                .regions
                .iter()
                .filter(|region| region.assessment == ResultAssessment::Different)
                .count(),
            1
        );
        assert!(result.regions.iter().any(|region| region.region.values
            == BTreeMap::from([(0, true), (1, false)])
            && region.assessment == ResultAssessment::Different));
        assert!(result.regions.iter().any(|region| region.region.values
            == BTreeMap::from([(0, false)])
            && region.assessment == ResultAssessment::Equal));
        assert_eq!(result.comparison_coverage, Coverage::Complete);
    }

    #[test]
    fn ifds_fr002_equal_branch_values() {
        let before = "function same(flag: boolean) { if (flag) return 1; return 1; }";
        let after = "function same(flag: boolean) { if (!flag) return 1; return 1; }";
        let entry = entry(&[(0, PrimitiveDomain::Boolean)]);
        let result = compare_sources(before, after, entry.clone(), entry);
        assert_eq!(result.regions.len(), 1, "{result:#?}");
        assert_eq!(result.regions[0].assessment, ResultAssessment::Equal);
        assert!(result.regions[0].region.values.is_empty());
        assert!(result.regions[0].control_changed);
    }

    #[test]
    fn ifds_fr002_early_return_and_undefined() {
        let before = "function status(flag: boolean) { return 2; }";
        let after = "function status(flag: boolean) { if (flag) return 1; return 2; }";
        let entry = entry(&[(0, PrimitiveDomain::Boolean)]);
        let result = compare_sources(before, after, entry.clone(), entry.clone());
        assert_eq!(result.regions.len(), 2, "{result:#?}");
        assert!(
            result
                .regions
                .iter()
                .any(|region| region.region.values.get(&0) == Some(&true)
                    && region.assessment == ResultAssessment::Different)
        );
        assert!(
            result
                .regions
                .iter()
                .any(|region| region.region.values.get(&0) == Some(&false)
                    && region.assessment == ResultAssessment::Equal)
        );
        let before = "function status(flag: boolean) { if (flag) return 1; }";
        let second = compare_sources(before, after, entry.clone(), entry);
        assert!(
            second
                .regions
                .iter()
                .any(|region| region.region.values.get(&0) == Some(&false)
                    && region.before_result.as_deref() == Some("undefined")
                    && region.assessment == ResultAssessment::Different),
            "{second:#?}"
        );
    }

    #[test]
    fn ifds_fr002_return_shape() {
        let before = "function value(flag: boolean) { return flag ? 1 : 2; }";
        let after = "function value(flag: boolean) { if (flag) return 1; return 2; }";
        let entry = entry(&[(0, PrimitiveDomain::Boolean)]);
        let result = compare_sources(before, after, entry.clone(), entry);
        assert!(
            result
                .regions
                .iter()
                .all(|region| region.assessment == ResultAssessment::Equal),
            "{result:#?}"
        );
        assert_eq!(result.source_alignment_coverage, Coverage::Partial);
    }

    #[test]
    fn ifds_fr002_same_input_and_copy() {
        let before = "function echo(input: string) { return input; }";
        let after = "function echo(input: string) { const copy = input; return copy; }";
        let domain = entry(&[(0, PrimitiveDomain::String)]);
        let equal = compare_sources(before, after, domain.clone(), domain.clone());
        assert_eq!(
            equal.regions[0].assessment,
            ResultAssessment::Equal,
            "{equal:#?}"
        );
        assert!(equal.regions[0].return_structure_changed);
        assert!(!equal.regions[0].value_dependency_changed);
        let before = "function choose(p: string, q: string) { return p; }";
        let after = "function choose(p: string, q: string) { return q; }";
        let entry = entry(&[(0, PrimitiveDomain::String), (1, PrimitiveDomain::String)]);
        let changed = compare_sources(before, after, entry.clone(), entry);
        assert_eq!(
            changed.regions[0].assessment,
            ResultAssessment::Changed,
            "{changed:#?}"
        );
    }

    #[test]
    fn ifds_fr002_guard_changes_return() {
        let before = "function value(flag: boolean) { if (flag) return \"yes\"; return \"no\"; }";
        let after = "function value(flag: boolean) { if (!flag) return \"yes\"; return \"no\"; }";
        let entry = FunctionEntry::default();
        let result = compare_sources(before, after, entry.clone(), entry);
        assert_eq!(result.regions.len(), 2, "{result:#?}");
        assert!(result.regions.iter().all(|region| region.assessment
            == ResultAssessment::Different
            && region.control_changed));
    }

    #[test]
    fn ifds_fr002_declared_input_scope() {
        let before = "function answer(flag: boolean) { if (flag) return 1; return 0; }";
        let after = "function answer(flag: boolean) { if (flag) return 2; return 0; }";
        let mut entry = entry(&[(0, PrimitiveDomain::Boolean)]);
        entry
            .known_values
            .insert(0, FunctionKnownValue::Boolean { value: true });
        let result = compare_sources(before, after, entry.clone(), entry);
        assert_eq!(result.regions.len(), 1, "{result:#?}");
        assert_eq!(result.regions[0].assessment, ResultAssessment::Different);
        assert_eq!(result.regions[0].region.values.get(&0), Some(&true));
        assert_eq!(result.outside_both_domain[0].values.get(&0), Some(&false));
    }

    #[test]
    fn ifds_fr002_entry_assumption_change() {
        let source = "function answer(flag: boolean, ready: boolean) { return 1; }";
        let mut before = entry(&[(0, PrimitiveDomain::Boolean), (1, PrimitiveDomain::Boolean)]);
        before.assumptions.push(AssumptionExpr::Binary {
            operator: AssumptionBinary::And,
            left: Box::new(AssumptionExpr::Input { index: 0 }),
            right: Box::new(AssumptionExpr::Input { index: 1 }),
        });
        let mut after = entry(&[(0, PrimitiveDomain::Boolean), (1, PrimitiveDomain::Boolean)]);
        after.assumptions.push(AssumptionExpr::Input { index: 0 });
        let result = compare_sources(source, source, before, after);
        assert_eq!(result.regions.len(), 1, "{result:#?}");
        assert_eq!(result.regions[0].assessment, ResultAssessment::Equal);
        assert_eq!(
            result.regions[0].region.values,
            BTreeMap::from([(0, true), (1, true)])
        );
        assert_eq!(
            result.after_only_domain[0].values,
            BTreeMap::from([(0, true), (1, false)])
        );
        assert_eq!(
            result.outside_both_domain[0].values,
            BTreeMap::from([(0, false)])
        );
    }

    #[test]
    fn ifds_fr002_independent_unknown_region() {
        let before = "function mixed(change: boolean, unsupported: boolean) { if (change) return 0; mystery(unsupported); return 1; }";
        let after = "function mixed(change: boolean, unsupported: boolean) { if (change) return 2; mystery(unsupported); return 1; }";
        let entry = entry(&[(0, PrimitiveDomain::Boolean), (1, PrimitiveDomain::Boolean)]);
        let result = compare_sources(before, after, entry.clone(), entry);
        assert!(
            result
                .regions
                .iter()
                .any(|region| region.region.values.get(&0) == Some(&true)
                    && region.assessment == ResultAssessment::Different),
            "{result:#?}"
        );
        assert!(
            result
                .regions
                .iter()
                .any(|region| region.region.values.get(&0) == Some(&false)
                    && region.assessment == ResultAssessment::Unknown)
        );
        assert_eq!(result.comparison_coverage, Coverage::Partial);
    }

    #[test]
    fn ifds_fr002_guard_versions_and_order() {
        let before = "function selected(flag: boolean) { let guard = flag; if (guard) return \"yes\"; return \"no\"; }";
        let after = "function selected(flag: boolean) { let guard = flag; guard = !guard; if (guard) return \"yes\"; return \"no\"; }";
        let domain = entry(&[(0, PrimitiveDomain::Boolean)]);
        let result = compare_sources(before, after, domain.clone(), domain);
        assert_eq!(result.regions.len(), 2, "{result:#?}");
        assert!(
            result
                .regions
                .iter()
                .all(|region| region.assessment == ResultAssessment::Different)
        );

        let before = "function priority(a: boolean, b: boolean) { let result = 0; if (a) result = 1; if (b) result = 2; return result; }";
        let after = "function priority(a: boolean, b: boolean) { let result = 0; if (b) result = 2; if (a) result = 1; return result; }";
        let entry = entry(&[(0, PrimitiveDomain::Boolean), (1, PrimitiveDomain::Boolean)]);
        let result = compare_sources(before, after, entry.clone(), entry);
        assert!(
            result.regions.iter().any(|region| region.region.values
                == BTreeMap::from([(0, true), (1, true)])
                && region.assessment == ResultAssessment::Different),
            "{result:#?}"
        );
        assert!(
            result
                .regions
                .iter()
                .filter(|region| region.assessment == ResultAssessment::Different)
                .count()
                == 1
        );
    }

    #[test]
    fn ifds_fr002_common_domain() {
        let source = "function enabled(flag: boolean) { if (flag) return 1; return 0; }";
        let before = entry(&[(0, PrimitiveDomain::Boolean)]);
        let mut after = before.clone();
        after
            .known_values
            .insert(0, FunctionKnownValue::Boolean { value: true });
        let result = compare_sources(source, source, before, after);
        assert_eq!(result.regions.len(), 1, "{result:#?}");
        assert_eq!(result.regions[0].assessment, ResultAssessment::Equal);
        assert_eq!(result.regions[0].region.values.get(&0), Some(&true));
        assert_eq!(result.before_only_domain[0].values.get(&0), Some(&false));
    }

    #[test]
    fn ifds_fr002_primitive_semantics() {
        let string = FunctionKnownValue::String { value: "x".into() };
        let number = FunctionKnownValue::Number { value: "1".into() };
        let boolean = FunctionKnownValue::Boolean { value: true };
        assert_eq!(
            literal_relation(&string, &string),
            Some(ResultAssessment::Equal)
        );
        assert_eq!(
            literal_relation(&number, &boolean),
            Some(ResultAssessment::Different)
        );
        assert_eq!(
            literal_relation(
                &FunctionKnownValue::Number {
                    value: "NaN".into()
                },
                &FunctionKnownValue::Number {
                    value: "NaN".into()
                }
            ),
            None
        );
        assert_eq!(
            literal_relation(
                &FunctionKnownValue::Number { value: "0".into() },
                &FunctionKnownValue::Number { value: "-0".into() }
            ),
            None
        );
        assert_eq!(
            literal_relation(
                &FunctionKnownValue::Number {
                    value: "NaN".into()
                },
                &boolean
            ),
            None
        );
    }

    #[test]
    fn ifds_fr002_presence_not_value() {
        let before = "function oldResult() { return undefined; }";
        let result = compare_sources(
            before,
            "",
            FunctionEntry::default(),
            FunctionEntry::default(),
        );
        assert_eq!(result.function_presence, FunctionPresence::BeforeOnly);
        assert!(result.regions.is_empty());
        assert_eq!(result.before_flow.len(), 1);
        assert!(result.after_flow.is_empty());
    }

    #[test]
    fn ifds_fr002_empty_common_domain() {
        let before = "function answer(flag: boolean) { if (flag) return \"old\"; return \"off\"; }";
        let after = "function answer(flag: boolean) { if (flag) return \"new\"; return \"off\"; }";
        let mut before_entry = entry(&[(0, PrimitiveDomain::Boolean)]);
        before_entry
            .known_values
            .insert(0, FunctionKnownValue::Boolean { value: true });
        let mut after_entry = entry(&[(0, PrimitiveDomain::Boolean)]);
        after_entry
            .known_values
            .insert(0, FunctionKnownValue::Boolean { value: false });
        let result = compare_sources(before, after, before_entry, after_entry);
        assert_eq!(result.common_domain_status, CommonDomainStatus::Empty);
        assert!(result.regions.is_empty());
        assert_eq!(result.before_flow.len(), 1);
        assert_eq!(result.after_flow.len(), 1);
        assert_eq!(result.before_flow[0].result.as_deref(), Some("\"old\""));
        assert_eq!(result.after_flow[0].result.as_deref(), Some("\"off\""));
        assert_eq!(result.before_observations.len(), 1);
        assert_eq!(result.after_observations.len(), 1);
        assert!(!result.before_observations[0].guards.is_empty());
        assert!(!result.after_observations[0].guards.is_empty());

        let source = "function answer(input: number) { return input; }";
        let mut before_entry = entry(&[(0, PrimitiveDomain::Number)]);
        before_entry
            .known_values
            .insert(0, FunctionKnownValue::Number { value: "1".into() });
        let mut after_entry = entry(&[(0, PrimitiveDomain::Number)]);
        after_entry
            .known_values
            .insert(0, FunctionKnownValue::Number { value: "2".into() });
        let disjoint = compare_sources(source, source, before_entry, after_entry);
        assert_eq!(disjoint.common_domain_status, CommonDomainStatus::Empty);
        assert!(disjoint.regions.is_empty());
        assert_eq!(disjoint.before_flow.len(), 1);
        assert_eq!(disjoint.after_flow.len(), 1);
        assert_ne!(
            disjoint.before_entry.known_values,
            disjoint.after_entry.known_values
        );
    }

    #[test]
    fn ifds_fr002_expression_changed() {
        let before = "function add(input: number) { return input + 1; }";
        let after = "function add(input: number) { return input + 2; }";
        let entry = entry(&[(0, PrimitiveDomain::Number)]);
        for scope in [FunctionEntry::default(), entry.clone()] {
            let changed = compare_sources(before, after, scope.clone(), scope);
            assert_eq!(changed.regions.len(), 1, "{changed:#?}");
            assert_eq!(changed.regions[0].assessment, ResultAssessment::Changed);
            assert_eq!(
                changed.regions[0].proof,
                Some(ResultProof::ChangedComputation)
            );
            assert_eq!(changed.comparison_coverage, Coverage::Complete);
        }
        let equal = compare_sources(before, before, entry.clone(), entry);
        assert_eq!(
            equal.regions[0].assessment,
            ResultAssessment::Equal,
            "{equal:#?}"
        );
    }

    #[test]
    fn changed_flow_between_two_inputs_requires_no_runtime_type_or_value_proof() {
        let before = "function sum(a, b, c) { const saved = a + b; a = 100; return saved; }";
        for after in [
            "function sum(a, b, c) { const saved = a - b; a = 100; return saved; }",
            "function sum(a, b, c) { const saved = a + c; a = 100; return saved; }",
        ] {
            let result = compare_sources(
                before,
                after,
                FunctionEntry::default(),
                FunctionEntry::default(),
            );
            assert_eq!(result.regions.len(), 1, "{result:#?}");
            let region = &result.regions[0];
            assert_eq!(region.assessment, ResultAssessment::Changed);
            assert_eq!(region.proof, Some(ResultProof::ChangedComputation));
            assert!(region.value_dependency_changed);
            assert_eq!(region.evidence, EvidenceKind::Supported);
            assert!(region.unknown_reason.is_none());
            assert_eq!(result.comparison_coverage, Coverage::Complete);
        }
        let overwritten = "function sum(a, b, c) { let saved = a + b; saved = 100; return saved; }";
        let result = compare_sources(
            overwritten,
            &overwritten.replace("a + b", "a + c"),
            FunctionEntry::default(),
            FunctionEntry::default(),
        );
        assert_eq!(result.regions[0].assessment, ResultAssessment::Equal);
        assert!(!result.regions[0].value_dependency_changed);
    }

    #[test]
    fn ifds_fr002_unknown_call_argument() {
        let before = "function value(p: string, q: string) { return mystery(p); }";
        let after = "function value(p: string, q: string) { return mystery(q); }";
        let entry = entry(&[(0, PrimitiveDomain::String), (1, PrimitiveDomain::String)]);
        let analysis = analyze_sources(before, after, entry.clone(), entry);
        let result = compare_function_results(&analysis);
        assert_eq!(result.regions.len(), 1, "{result:#?}");
        assert_eq!(result.regions[0].assessment, ResultAssessment::Unknown);
        assert!(result.regions[0].value_dependency_changed, "{result:#?}");
        assert!(result.regions[0].return_structure_changed);
    }

    #[test]
    fn ifds_fr002_unknown_completion() {
        let before = "function value() { return 1; }";
        let after = "function value() { mystery(); return 1; }";
        let result = compare_sources(
            before,
            after,
            FunctionEntry::default(),
            FunctionEntry::default(),
        );
        assert_eq!(
            result.regions[0].assessment,
            ResultAssessment::Unknown,
            "{result:#?}"
        );
        assert_eq!(
            result.regions[0].evidence,
            EvidenceKind::Unresolved {
                diagnostic: DiagnosticCode::UnresolvedCall
            }
        );
        assert_eq!(result.after_flow[0].completion, Coverage::Partial);
    }

    #[test]
    fn ifds_fr002_identical_unsupported_guard() {
        let source = "function value(p: string) { if (mystery(p)) return 1; return 2; }";
        let entry = entry(&[(0, PrimitiveDomain::String)]);
        let result = compare_sources(source, source, entry.clone(), entry);
        assert!(
            result
                .regions
                .iter()
                .all(|region| region.assessment == ResultAssessment::Unknown),
            "{result:#?}"
        );
    }

    #[test]
    fn ifds_fr002_untyped_strict_equality_stays_unknown() {
        let source = "function value(p: unknown, q: unknown) { if (p === q) return 1; return 2; }";
        let result = compare_sources(
            source,
            source,
            FunctionEntry::default(),
            FunctionEntry::default(),
        );
        assert!(
            result
                .regions
                .iter()
                .all(|region| region.assessment == ResultAssessment::Unknown),
            "{result:#?}"
        );
        assert_eq!(result.comparison_coverage, Coverage::Partial);
    }

    #[test]
    fn ifds_fr002_disjoint_regions() {
        let input = vec![
            vec![false, false, false],
            vec![false, false, true],
            vec![false, true, false],
            vec![false, true, true],
            vec![true, false, false],
            vec![true, false, true],
        ];
        let cubes = merge_cubes(
            input
                .iter()
                .map(|bits| bits.iter().copied().map(Some).collect())
                .collect(),
        );
        let contains = |cube: &Vec<Option<bool>>, bits: &Vec<bool>| {
            cube.iter()
                .zip(bits)
                .all(|(part, bit)| part.is_none_or(|part| part == *bit))
        };
        for bits in &input {
            assert_eq!(cubes.iter().filter(|cube| contains(cube, bits)).count(), 1);
        }
        for bits in [vec![true, true, false], vec![true, true, true]] {
            assert!(!cubes.iter().any(|cube| contains(cube, &bits)));
        }
    }

    #[test]
    fn ifds_fr002_same_text_distinct_values() {
        let source = "function selected(flag: boolean) { let value = 0; if (flag) value = 1; return value; }";
        let entry = entry(&[(0, PrimitiveDomain::Boolean)]);
        let result = compare_sources(source, source, entry.clone(), entry);
        assert_eq!(result.regions.len(), 2, "{result:#?}");
        assert!(
            result
                .regions
                .iter()
                .all(|region| region.assessment == ResultAssessment::Equal)
        );
    }
}
