//! Single-function normal-result observations and their upstream dependencies.

use super::adapters::typescript::{
    IndexedFunction, index_bindings, index_functions, lower_function_procedure,
};
use super::branches::{BranchPaths, BranchState};
use super::ir::{ComputeInputRole, EdgeKind, Operation, PrimitiveOperator, ProcedureIr};
use super::model::*;
use super::provenance::seed_entry_facts;
use super::snapshots::{AnalysisEnvironment, SnapshotProvider, validate_query};
use super::solver::{
    IntraproceduralSolver, Solver, SolverRequest, SolverTermination, SystemSolverControl,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

pub const FUNCTION_RESULT_SCHEMA_VERSION: u32 = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionSelector {
    pub snapshot: SnapshotId,
    pub declaration: SourceSpan,
    pub expected_name: Option<String>,
    pub expected_enclosing_symbol: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveDomain {
    Boolean,
    Number,
    String,
    Nullish,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FunctionKnownValue {
    Boolean { value: bool },
    Number { value: String },
    String { value: String },
    Null,
    Undefined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssumptionUnary {
    Not,
    Plus,
    Minus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssumptionBinary {
    And,
    Or,
    Nullish,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    StrictEqual,
    StrictNotEqual,
    GreaterThan,
    LessThan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssumptionExpr {
    Input {
        index: u32,
    },
    Literal {
        value: FunctionKnownValue,
    },
    Unary {
        operator: AssumptionUnary,
        operand: Box<Self>,
    },
    Binary {
        operator: AssumptionBinary,
        left: Box<Self>,
        right: Box<Self>,
    },
    Conditional {
        condition: Box<Self>,
        when_true: Box<Self>,
        when_false: Box<Self>,
    },
    Unsupported {
        source: String,
        reason: String,
        inputs: BTreeSet<u32>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionEntry {
    pub domains: BTreeMap<u32, PrimitiveDomain>,
    pub known_values: BTreeMap<u32, FunctionKnownValue>,
    pub assumptions: Vec<AssumptionExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionResultQuery {
    pub before: SnapshotHandle,
    pub after: SnapshotHandle,
    pub diff: RepositoryDiff,
    pub selected_function: FunctionSelector,
    pub counterpart: Option<FunctionSelector>,
    pub before_entry: FunctionEntry,
    pub after_entry: FunctionEntry,
    pub capabilities: CapabilitySet,
    pub summaries: BTreeSet<ModelVersion>,
    pub limits: AnalysisLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionCounterpart {
    Matched,
    ConfirmedAbsent,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputCorrespondence {
    pub position: u32,
    pub before: BindingId,
    pub after: BindingId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultDependency {
    pub node: NodeId,
    pub span: Option<SourceSpan>,
    pub role: Option<ComputeInputRole>,
    pub operation: String,
    pub literal: Option<FunctionKnownValue>,
    pub operator: Option<PrimitiveOperator>,
    pub origin: Option<Source>,
    pub inputs: Vec<Self>,
    pub unresolved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultGuard {
    pub branch: NodeId,
    pub span: Option<SourceSpan>,
    pub outcome: bool,
    pub condition: ResultDependency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResultValue {
    Undefined,
    Expression {
        text: String,
        span: SourceSpan,
        dependency: Box<ResultDependency>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultExitKind {
    Explicit,
    Bare,
    Arrow,
    Fallthrough,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultObservation {
    pub site: NodeId,
    pub path_index: u32,
    pub kind: ResultExitKind,
    pub span: SourceSpan,
    pub effective_condition: Option<String>,
    pub guards: Vec<ResultGuard>,
    pub value: ResultValue,
    pub unknown_completion_before_return: bool,
    pub dependency_coverage: Coverage,
    pub caller_continuation_open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionSnapshotResult {
    pub function: FunctionSelector,
    pub procedure: ProcedureId,
    pub parameters: Vec<BindingId>,
    pub parameter_names: Vec<String>,
    pub parameter_forms_supported: bool,
    pub entry: FunctionEntry,
    pub observations: Vec<ResultObservation>,
    pub unknown_boundaries: BTreeSet<NodeId>,
    pub diagnostics: BTreeSet<Diagnostic>,
    pub unknown_frontiers: BTreeSet<UnknownFrontier>,
    pub coverage: Coverage,
    pub limits_hit: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionResultAnalysis {
    pub schema_version: u32,
    pub query: FunctionResultQuery,
    pub counterpart_status: FunctionCounterpart,
    pub input_correspondence: Vec<InputCorrespondence>,
    pub input_mapping_coverage: Coverage,
    pub before: Option<FunctionSnapshotResult>,
    pub after: Option<FunctionSnapshotResult>,
}

#[derive(Debug)]
pub enum FunctionResultError {
    Input(InputError),
    Adapter(String),
    Solver(String),
}

impl fmt::Display for FunctionResultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "function-result analysis failed: {self:?}")
    }
}
impl std::error::Error for FunctionResultError {}
impl From<InputError> for FunctionResultError {
    fn from(value: InputError) -> Self {
        Self::Input(value)
    }
}

fn validation_query(query: &FunctionResultQuery) -> VariableFlowQuery {
    let binding = |selector: &FunctionSelector| BindingSelector {
        snapshot: selector.snapshot.clone(),
        declaration: selector.declaration.clone(),
        expected_name: selector.expected_name.clone(),
        expected_enclosing_symbol: selector.expected_enclosing_symbol.clone(),
    };
    VariableFlowQuery {
        before: query.before.clone(),
        after: query.after.clone(),
        diff: query.diff.clone(),
        selected_binding: binding(&query.selected_function),
        counterpart: query.counterpart.as_ref().map(binding),
        entry: EntryPoint::ContainingFunction,
        capabilities: query.capabilities.clone(),
        summaries: query.summaries.clone(),
        limits: query.limits.clone(),
    }
}

fn read_source<P: SnapshotProvider>(
    provider: &P,
    handle: &SnapshotHandle,
    path: &str,
) -> Result<String, FunctionResultError> {
    String::from_utf8(provider.read_file(handle, path)?)
        .map_err(|error| FunctionResultError::Adapter(format!("{path}: {error}")))
}

fn select<'a>(
    functions: &'a [IndexedFunction],
    selector: &FunctionSelector,
) -> Result<&'a IndexedFunction, FunctionResultError> {
    let mut matches = functions
        .iter()
        .filter(|function| function.span == selector.declaration);
    let function = matches.next().ok_or_else(|| {
        FunctionResultError::Input(InputError::InvalidSelector(
            "span does not identify an ordinary function declaration, expression, or arrow".into(),
        ))
    })?;
    if matches.next().is_some()
        || selector
            .expected_name
            .as_ref()
            .is_some_and(|name| function.name.as_ref() != Some(name))
        || selector
            .expected_enclosing_symbol
            .as_ref()
            .is_some_and(|owner| function.owner.as_ref() != Some(owner))
    {
        return Err(FunctionResultError::Input(InputError::InvalidSelector(
            "ambiguous or stale function selector".into(),
        )));
    }
    Ok(function)
}

fn inferred_path(query: &FunctionResultQuery, selected: &FunctionSelector) -> String {
    query
        .diff
        .changes
        .iter()
        .find_map(|change| match selected.snapshot.side {
            SnapshotSide::Before
                if change.before_path.as_deref() == Some(&selected.declaration.path) =>
            {
                change.after_path.clone()
            }
            SnapshotSide::After
                if change.after_path.as_deref() == Some(&selected.declaration.path) =>
            {
                change.before_path.clone()
            }
            _ => None,
        })
        .unwrap_or_else(|| selected.declaration.path.clone())
}

/// Analyze each selected function independently; FR002 owns cross-version value relations.
pub fn analyze_function_result<P: SnapshotProvider>(
    query: FunctionResultQuery,
    provider: &P,
    before_environment: &AnalysisEnvironment,
    after_environment: &AnalysisEnvironment,
) -> Result<FunctionResultAnalysis, FunctionResultError> {
    validate_query(
        &validation_query(&query),
        provider,
        before_environment,
        after_environment,
    )?;
    if let Some(counterpart) = &query.counterpart
        && counterpart.snapshot.side == query.selected_function.snapshot.side
    {
        return Err(InputError::InvalidSelector(
            "function selectors must use opposite sides".into(),
        )
        .into());
    }
    let selected_handle = if query.selected_function.snapshot.side == SnapshotSide::Before {
        &query.before
    } else {
        &query.after
    };
    let other_handle = if selected_handle.id.side == SnapshotSide::Before {
        &query.after
    } else {
        &query.before
    };
    let selected_source = read_source(
        provider,
        selected_handle,
        &query.selected_function.declaration.path,
    )?;
    let selected_index =
        index_functions(&query.selected_function.declaration.path, &selected_source)
            .map_err(|error| FunctionResultError::Adapter(error.to_string()))?;
    let selected = select(&selected_index, &query.selected_function)?.clone();
    let other_path = query
        .counterpart
        .as_ref()
        .map(|value| value.declaration.path.clone())
        .unwrap_or_else(|| inferred_path(&query, &query.selected_function));
    let other_files = provider.files(other_handle)?;
    let (other, counterpart_status) = if other_files.contains_key(&other_path) {
        let source = read_source(provider, other_handle, &other_path)?;
        let functions = index_functions(&other_path, &source)
            .map_err(|error| FunctionResultError::Adapter(error.to_string()))?;
        if let Some(explicit) = &query.counterpart {
            (
                Some(select(&functions, explicit)?.clone()),
                FunctionCounterpart::Matched,
            )
        } else {
            let candidates: Vec<_> = functions
                .iter()
                .filter(|candidate| {
                    candidate.name == selected.name && candidate.owner == selected.owner
                })
                .cloned()
                .collect();
            if candidates.len() == 1 && selected.name.is_some() {
                (candidates.into_iter().next(), FunctionCounterpart::Matched)
            } else if functions.is_empty() {
                (None, FunctionCounterpart::ConfirmedAbsent)
            } else {
                (None, FunctionCounterpart::Unresolved)
            }
        }
    } else if query.counterpart.is_some() {
        return Err(
            InputError::InvalidSelector("explicit counterpart file is absent".into()).into(),
        );
    } else {
        (None, FunctionCounterpart::ConfirmedAbsent)
    };
    let selected_result = analyze_side(
        provider,
        selected_handle,
        &selected,
        &query,
        if selected_handle.id.side == SnapshotSide::Before {
            &query.before_entry
        } else {
            &query.after_entry
        },
    )?;
    let other_result = if let Some(other) = &other {
        Some(analyze_side(
            provider,
            other_handle,
            other,
            &query,
            if other_handle.id.side == SnapshotSide::Before {
                &query.before_entry
            } else {
                &query.after_entry
            },
        )?)
    } else {
        None
    };
    let (before, after) = if selected_handle.id.side == SnapshotSide::Before {
        (Some(selected_result), other_result)
    } else {
        (other_result, Some(selected_result))
    };
    let input_correspondence = before
        .as_ref()
        .zip(after.as_ref())
        .map(|(before, after)| {
            before
                .parameters
                .iter()
                .zip(&after.parameters)
                .enumerate()
                .map(|(position, (before, after))| InputCorrespondence {
                    position: position as u32,
                    before: before.clone(),
                    after: after.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let input_mapping_coverage = match before.as_ref().zip(after.as_ref()) {
        Some((before, after))
            if before.parameter_forms_supported
                && after.parameter_forms_supported
                && before.parameters.len() == after.parameters.len() =>
        {
            Coverage::Complete
        }
        _ => Coverage::Partial,
    };
    Ok(FunctionResultAnalysis {
        schema_version: FUNCTION_RESULT_SCHEMA_VERSION,
        query,
        counterpart_status,
        input_correspondence,
        input_mapping_coverage,
        before,
        after,
    })
}

fn analyze_side<P: SnapshotProvider>(
    provider: &P,
    handle: &SnapshotHandle,
    function: &IndexedFunction,
    query: &FunctionResultQuery,
    entry: &FunctionEntry,
) -> Result<FunctionSnapshotResult, FunctionResultError> {
    let source = read_source(provider, handle, &function.span.path)?;
    let index = index_bindings(handle.id.clone(), &function.span.path, &source)
        .map_err(|error| FunctionResultError::Adapter(error.to_string()))?;
    let (mut procedure, mut diagnostics) =
        lower_function_procedure(&source, &index, &function.span)
            .map_err(|error| FunctionResultError::Adapter(error.to_string()))?;
    // A nested declaration does not run its body. Keep binding-query capture
    // frontiers unchanged while treating the declaration as inert for this target.
    let inert: BTreeSet<_> = procedure
        .nodes
        .values_mut()
        .filter_map(|node| {
            let Operation::UnknownEffect { description, .. } = &node.operation else {
                return None;
            };
            if description.contains("function_declaration")
                || description.contains("generator_function_declaration")
            {
                node.operation = Operation::Join;
                Some(node.id.clone())
            } else {
                None
            }
        })
        .collect();
    diagnostics.retain(|diagnostic| {
        !diagnostic
            .frontier
            .as_ref()
            .is_some_and(|id| inert.contains(id))
    });
    validate_entry(entry)?;
    let parameter_forms_supported = !index.diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains("parameter")
            && diagnostic.span.as_ref().is_some_and(|span| {
                span.byte_start >= function.span.byte_start
                    && span.byte_end <= function.span.byte_end
            })
    });
    let solver = IntraproceduralSolver::new();
    let solved = solver
        .solve(
            SolverRequest {
                procedure: &procedure,
                entry_facts: seed_entry_facts(&procedure),
                limits: query.limits.clone(),
            },
            &SystemSolverControl::new(),
        )
        .map_err(|error| FunctionResultError::Solver(error.to_string()))?;
    let paths = BranchPaths::build(
        &procedure,
        seed_entry_facts(&procedure),
        usize::try_from(query.limits.processed_path_edges.min(4096))
            .unwrap_or(4096)
            .max(1),
    );
    let selector = FunctionSelector {
        snapshot: handle.id.clone(),
        declaration: function.span.clone(),
        expected_name: function.name.clone(),
        expected_enclosing_symbol: function.owner.clone(),
    };
    let context = ObservationContext {
        procedure: &procedure,
        paths: &paths,
        source: &source,
        function_span: &function.span,
    };
    let mut observations = Vec::new();
    for node in procedure.nodes.values() {
        let Operation::Return { value } = &node.operation else {
            continue;
        };
        let Some(states) = paths.at.get(&node.id) else {
            continue;
        };
        for (path_index, state) in states.iter().enumerate() {
            match entry_viability(entry, &procedure, &paths, state) {
                EntryViability::Excluded => {}
                viability => {
                    let mut observation =
                        observe_return(&context, node, value.as_ref(), state, path_index as u32);
                    if viability == EntryViability::Unresolved {
                        observation.dependency_coverage = Coverage::Partial;
                    }
                    observations.push(observation);
                }
            }
        }
    }
    for exit in &procedure.exits {
        for edge in procedure
            .edges
            .iter()
            .filter(|edge| &edge.target == exit && edge.kind == EdgeKind::Normal)
        {
            if matches!(
                procedure.nodes[&edge.source].operation,
                Operation::Return { .. }
            ) {
                continue;
            }
            if let Some(states) = paths.at.get(&edge.source) {
                for (path_index, state) in states.iter().enumerate() {
                    match entry_viability(entry, &procedure, &paths, state) {
                        EntryViability::Excluded => {}
                        viability => {
                            let mut observation = observe_fallthrough(
                                &context,
                                &edge.source,
                                state,
                                path_index as u32,
                            );
                            if viability == EntryViability::Unresolved {
                                observation.dependency_coverage = Coverage::Partial;
                            }
                            observations.push(observation);
                        }
                    }
                }
            }
        }
    }
    observations.sort_by(|a, b| (&a.site, a.path_index).cmp(&(&b.site, b.path_index)));
    let unknown_boundaries: BTreeSet<NodeId> = procedure
        .nodes
        .values()
        .filter(|node| {
            matches!(
                node.operation,
                Operation::UnknownEffect { .. } | Operation::Call { .. }
            ) && paths.at.get(&node.id).is_some_and(|states| {
                states.iter().any(|state| {
                    entry_viability(entry, &procedure, &paths, state) != EntryViability::Excluded
                })
            })
        })
        .map(|node| node.id.clone())
        .collect();
    let mut report_diagnostics = solved.diagnostics.clone();
    report_diagnostics.extend(diagnostics.iter().cloned());
    report_diagnostics.extend(
        index
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.message.contains("parameter")
                    && diagnostic.span.as_ref().is_some_and(|span| {
                        span.byte_start >= function.span.byte_start
                            && span.byte_end <= function.span.byte_end
                    })
            })
            .cloned(),
    );
    let unknown_frontiers = solved
        .unknown_frontiers
        .iter()
        .filter(|frontier| unknown_boundaries.contains(&frontier.node))
        .cloned()
        .collect();
    let mut limits_hit = solved.stats.limits_hit.clone();
    if paths.truncated {
        limits_hit.insert("branch_states".into());
    }
    let coverage = if !matches!(solved.termination, SolverTermination::FixedPoint)
        || paths.truncated
        || !unknown_boundaries.is_empty()
        || !parameter_forms_supported
        || observations
            .iter()
            .any(|observation| observation.dependency_coverage != Coverage::Complete)
        || diagnostics.iter().any(|diagnostic| {
            diagnostic.frontier.as_ref().is_some_and(|id| {
                paths.at.get(id).is_some_and(|states| {
                    states.iter().any(|state| {
                        entry_viability(entry, &procedure, &paths, state)
                            != EntryViability::Excluded
                    })
                })
            })
        }) {
        Coverage::Partial
    } else {
        Coverage::Complete
    };
    Ok(FunctionSnapshotResult {
        function: selector,
        procedure: procedure.id.clone(),
        parameters: procedure
            .parameters
            .iter()
            .map(|parameter| parameter.binding.clone())
            .collect(),
        parameter_names: procedure
            .parameters
            .iter()
            .map(|parameter| {
                index
                    .bindings
                    .iter()
                    .find(|binding| binding.id == parameter.binding)
                    .map_or_else(
                        || format!("arg{}", parameter.index),
                        |binding| binding.name.clone(),
                    )
            })
            .collect(),
        parameter_forms_supported,
        entry: entry.clone(),
        observations,
        unknown_boundaries,
        diagnostics: report_diagnostics,
        unknown_frontiers,
        coverage,
        limits_hit,
    })
}

fn validate_entry(entry: &FunctionEntry) -> Result<(), FunctionResultError> {
    for (index, value) in &entry.known_values {
        if known_eval(value).is_none() {
            return Err(InputError::InvalidSelector(format!(
                "input {index} has an invalid known value"
            ))
            .into());
        }
        if let Some(domain) = entry.domains.get(index) {
            let valid = matches!(
                (domain, value),
                (PrimitiveDomain::Boolean, FunctionKnownValue::Boolean { .. })
                    | (PrimitiveDomain::Number, FunctionKnownValue::Number { .. })
                    | (PrimitiveDomain::String, FunctionKnownValue::String { .. })
                    | (
                        PrimitiveDomain::Nullish,
                        FunctionKnownValue::Null | FunctionKnownValue::Undefined
                    )
            );
            if !valid {
                return Err(InputError::InvalidSelector(format!(
                    "input {index} has a value outside its declared domain"
                ))
                .into());
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EntryViability {
    Supported,
    Excluded,
    Unresolved,
}

#[derive(Clone, PartialEq)]
enum EvalValue {
    Boolean(bool),
    Number(f64),
    String(String),
    Null,
    Undefined,
}

impl EvalValue {
    fn truthy(&self) -> bool {
        match self {
            Self::Boolean(value) => *value,
            Self::Number(value) => *value != 0.0 && !value.is_nan(),
            Self::String(value) => !value.is_empty(),
            Self::Null | Self::Undefined => false,
        }
    }
}

fn known_eval(value: &FunctionKnownValue) -> Option<EvalValue> {
    match value {
        FunctionKnownValue::Boolean { value } => Some(EvalValue::Boolean(*value)),
        FunctionKnownValue::Number { value } => {
            value.replace('_', "").parse().ok().map(EvalValue::Number)
        }
        FunctionKnownValue::String { value } => Some(EvalValue::String(value.clone())),
        FunctionKnownValue::Null => Some(EvalValue::Null),
        FunctionKnownValue::Undefined => Some(EvalValue::Undefined),
    }
}

fn eval_assumption(
    expression: &AssumptionExpr,
    values: &BTreeMap<u32, EvalValue>,
) -> Option<EvalValue> {
    match expression {
        AssumptionExpr::Input { index } => values.get(index).cloned(),
        AssumptionExpr::Literal { value } => known_eval(value),
        AssumptionExpr::Unary { operator, operand } => {
            let value = eval_assumption(operand, values)?;
            eval_unary(*operator, value)
        }
        AssumptionExpr::Binary {
            operator,
            left,
            right,
        } => {
            let left = eval_assumption(left, values)?;
            if *operator == AssumptionBinary::And && !left.truthy() {
                return Some(left);
            }
            if *operator == AssumptionBinary::Or && left.truthy() {
                return Some(left);
            }
            if *operator == AssumptionBinary::Nullish
                && !matches!(left, EvalValue::Null | EvalValue::Undefined)
            {
                return Some(left);
            }
            let right = eval_assumption(right, values)?;
            eval_binary(*operator, left, right)
        }
        AssumptionExpr::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            if eval_assumption(condition, values)?.truthy() {
                eval_assumption(when_true, values)
            } else {
                eval_assumption(when_false, values)
            }
        }
        AssumptionExpr::Unsupported { .. } => None,
    }
}

fn eval_unary(operator: AssumptionUnary, value: EvalValue) -> Option<EvalValue> {
    match (operator, value) {
        (AssumptionUnary::Not, value) => Some(EvalValue::Boolean(!value.truthy())),
        (AssumptionUnary::Plus, EvalValue::Number(value)) => Some(EvalValue::Number(value)),
        (AssumptionUnary::Minus, EvalValue::Number(value)) => Some(EvalValue::Number(-value)),
        _ => None,
    }
}

fn eval_binary(operator: AssumptionBinary, left: EvalValue, right: EvalValue) -> Option<EvalValue> {
    match operator {
        AssumptionBinary::And => Some(if left.truthy() { right } else { left }),
        AssumptionBinary::Or => Some(if left.truthy() { left } else { right }),
        AssumptionBinary::Nullish => {
            Some(if matches!(left, EvalValue::Null | EvalValue::Undefined) {
                right
            } else {
                left
            })
        }
        AssumptionBinary::StrictEqual => Some(EvalValue::Boolean(left == right)),
        AssumptionBinary::StrictNotEqual => Some(EvalValue::Boolean(left != right)),
        AssumptionBinary::Add => match (left, right) {
            (EvalValue::Number(a), EvalValue::Number(b)) => Some(EvalValue::Number(a + b)),
            (EvalValue::String(a), EvalValue::String(b)) => Some(EvalValue::String(a + &b)),
            _ => None,
        },
        AssumptionBinary::Subtract => numeric_binary(left, right, |a, b| a - b),
        AssumptionBinary::Multiply => numeric_binary(left, right, |a, b| a * b),
        AssumptionBinary::Divide => numeric_binary(left, right, |a, b| a / b),
        AssumptionBinary::Remainder => numeric_binary(left, right, |a, b| a % b),
        AssumptionBinary::GreaterThan => numeric_compare(left, right, |a, b| a > b),
        AssumptionBinary::LessThan => numeric_compare(left, right, |a, b| a < b),
    }
}

fn numeric_binary(
    left: EvalValue,
    right: EvalValue,
    f: impl Fn(f64, f64) -> f64,
) -> Option<EvalValue> {
    match (left, right) {
        (EvalValue::Number(a), EvalValue::Number(b)) => Some(EvalValue::Number(f(a, b))),
        _ => None,
    }
}

fn numeric_compare(
    left: EvalValue,
    right: EvalValue,
    f: impl Fn(f64, f64) -> bool,
) -> Option<EvalValue> {
    match (left, right) {
        (EvalValue::Number(a), EvalValue::Number(b)) => Some(EvalValue::Boolean(f(a, b))),
        _ => None,
    }
}

fn eval_dependency(
    dependency: &ResultDependency,
    procedure: &ProcedureIr,
    values: &BTreeMap<u32, EvalValue>,
    entry: &FunctionEntry,
) -> Option<EvalValue> {
    if let Some(Source::FunctionInput(binding)) = &dependency.origin {
        let index = procedure
            .parameters
            .iter()
            .find(|parameter| &parameter.binding == binding)?
            .index;
        return values.get(&index).cloned();
    }
    let node = procedure.nodes.get(&dependency.node)?;
    match &node.operation {
        Operation::Literal {
            literal_kind,
            raw,
            cooked,
            ..
        } => match literal_kind {
            super::ir::LiteralKind::Boolean => Some(EvalValue::Boolean(raw == "true")),
            super::ir::LiteralKind::Number => {
                raw.replace('_', "").parse().ok().map(EvalValue::Number)
            }
            super::ir::LiteralKind::String => cooked
                .clone()
                .or_else(|| raw.get(1..raw.len().checked_sub(1)?).map(str::to_owned))
                .map(EvalValue::String),
            super::ir::LiteralKind::Null => Some(EvalValue::Null),
            _ => None,
        },
        Operation::Compute { operator, .. } => {
            let input = |index: usize| {
                dependency
                    .inputs
                    .get(index)
                    .and_then(|value| eval_dependency(value, procedure, values, entry))
            };
            match operator {
                PrimitiveOperator::LogicalNot => eval_unary(AssumptionUnary::Not, input(0)?),
                PrimitiveOperator::IsNullish => input(0)
                    .map(|value| matches!(value, EvalValue::Null | EvalValue::Undefined))
                    .or_else(|| dependency_nullish(dependency.inputs.first()?, procedure, entry))
                    .map(EvalValue::Boolean),
                PrimitiveOperator::UnaryPlus => eval_unary(AssumptionUnary::Plus, input(0)?),
                PrimitiveOperator::UnaryMinus => eval_unary(AssumptionUnary::Minus, input(0)?),
                PrimitiveOperator::ValueJoin { .. } => input(0),
                other => {
                    let operator = match other {
                        PrimitiveOperator::Add => AssumptionBinary::Add,
                        PrimitiveOperator::Subtract => AssumptionBinary::Subtract,
                        PrimitiveOperator::Multiply => AssumptionBinary::Multiply,
                        PrimitiveOperator::Divide => AssumptionBinary::Divide,
                        PrimitiveOperator::Remainder => AssumptionBinary::Remainder,
                        PrimitiveOperator::StrictEqual => AssumptionBinary::StrictEqual,
                        PrimitiveOperator::StrictNotEqual => AssumptionBinary::StrictNotEqual,
                        PrimitiveOperator::GreaterThan => AssumptionBinary::GreaterThan,
                        PrimitiveOperator::LessThan => AssumptionBinary::LessThan,
                        _ => return None,
                    };
                    eval_binary(operator, input(0)?, input(1)?)
                }
            }
        }
        _ if dependency.inputs.len() == 1 => {
            eval_dependency(&dependency.inputs[0], procedure, values, entry)
        }
        _ => None,
    }
}

/// Nullishness of an unchanged input/copy follows from its declared domain;
/// proving this does not require evaluating an arbitrary number or string.
fn dependency_nullish(
    dependency: &ResultDependency,
    procedure: &ProcedureIr,
    entry: &FunctionEntry,
) -> Option<bool> {
    if dependency.unresolved {
        return None;
    }
    if let Some(Source::FunctionInput(binding)) = &dependency.origin {
        let index = procedure
            .parameters
            .iter()
            .find(|parameter| &parameter.binding == binding)?
            .index;
        return entry
            .domains
            .get(&index)
            .map(|domain| *domain == PrimitiveDomain::Nullish);
    }
    if let Some(literal) = &dependency.literal {
        return Some(matches!(
            literal,
            FunctionKnownValue::Null | FunctionKnownValue::Undefined
        ));
    }
    if (matches!(
        dependency.operation.as_str(),
        "read" | "write" | "binding_value"
    ) || matches!(
        dependency.operator,
        Some(PrimitiveOperator::ValueJoin { .. })
    )) && dependency.inputs.len() == 1
    {
        return dependency_nullish(&dependency.inputs[0], procedure, entry);
    }
    None
}

fn entry_viability(
    entry: &FunctionEntry,
    procedure: &ProcedureIr,
    paths: &BranchPaths,
    state: &BranchState,
) -> EntryViability {
    if entry.domains.is_empty() && entry.known_values.is_empty() && entry.assumptions.is_empty() {
        return EntryViability::Supported;
    }
    let slots: Vec<_> = entry
        .domains
        .iter()
        .filter_map(|(index, domain)| {
            (*domain == PrimitiveDomain::Boolean && !entry.known_values.contains_key(index))
                .then_some(*index)
        })
        .collect();
    if slots.len() > 12 {
        return EntryViability::Unresolved;
    }
    let mut base = BTreeMap::new();
    for (index, value) in &entry.known_values {
        let Some(value) = known_eval(value) else {
            return EntryViability::Unresolved;
        };
        base.insert(*index, value);
    }
    let mut unresolved = false;
    for assignment in 0..(1_usize << slots.len()) {
        let mut values = base.clone();
        for (bit, index) in slots.iter().enumerate() {
            values.insert(*index, EvalValue::Boolean(assignment & (1 << bit) != 0));
        }
        let mut uncertain = false;
        let mut rejected = false;
        for assumption in &entry.assumptions {
            match eval_assumption(assumption, &values) {
                Some(value) if !value.truthy() => {
                    rejected = true;
                    break;
                }
                Some(_) => {}
                None => uncertain = true,
            }
        }
        if rejected {
            continue;
        }
        let mut walker = DependencyWalker {
            procedure,
            paths,
            decisions: &state.decisions,
            visiting: BTreeSet::new(),
        };
        for guard in walker.guards(state) {
            match eval_dependency(&guard.condition, procedure, &values, entry) {
                Some(value) if value.truthy() != guard.outcome => {
                    rejected = true;
                    break;
                }
                Some(_) => {}
                None => uncertain = true,
            }
        }
        if rejected {
            continue;
        }
        if uncertain {
            unresolved = true;
        } else {
            return EntryViability::Supported;
        }
    }
    if unresolved {
        EntryViability::Unresolved
    } else {
        EntryViability::Excluded
    }
}

fn condition(state: &BranchState) -> Option<String> {
    (!state.labels.is_empty()).then(|| {
        state
            .labels
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(" && ")
    })
}

struct ObservationContext<'a> {
    procedure: &'a ProcedureIr,
    paths: &'a BranchPaths,
    source: &'a str,
    function_span: &'a SourceSpan,
}

fn observe_return(
    context: &ObservationContext<'_>,
    node: &super::ir::IrNode,
    value: Option<&Place>,
    state: &BranchState,
    path_index: u32,
) -> ResultObservation {
    let ObservationContext {
        procedure,
        paths,
        source,
        function_span,
    } = *context;
    let mut walker = DependencyWalker {
        procedure,
        paths,
        decisions: &state.decisions,
        visiting: BTreeSet::new(),
    };
    let guards = walker.guards(state);
    let result = if let Some(value) = value {
        let dependency = walker.trace(value, &node.id, None);
        let span = match value {
            Place::Temporary(id) => procedure
                .nodes
                .get(id)
                .and_then(|producer| producer.span.clone()),
            Place::Binding(_) => None,
        }
        .unwrap_or_else(|| node.span.clone().unwrap_or_else(|| function_span.clone()));
        let text = source
            .get(span.byte_start as usize..span.byte_end as usize)
            .unwrap_or("")
            .to_owned();
        ResultValue::Expression {
            text,
            span,
            dependency: Box::new(dependency),
        }
    } else {
        ResultValue::Undefined
    };
    let unknown_completion_before_return = unknown_before(procedure, &node.id, &state.decisions);
    let dependency_coverage = if unknown_completion_before_return
        || value_unresolved(&result)
        || guards.iter().any(|guard| guard.condition.unresolved)
    {
        Coverage::Partial
    } else {
        Coverage::Complete
    };
    let kind = if value.is_none() {
        ResultExitKind::Bare
    } else if node.span.as_ref().is_some_and(|span| {
        span.byte_start == function_span.byte_start
            || !source
                .get(span.byte_start as usize..span.byte_end as usize)
                .unwrap_or("")
                .trim_start()
                .starts_with("return")
    }) {
        ResultExitKind::Arrow
    } else {
        ResultExitKind::Explicit
    };
    ResultObservation {
        site: node.id.clone(),
        path_index,
        kind,
        span: node.span.clone().unwrap_or_else(|| function_span.clone()),
        effective_condition: condition(state),
        guards,
        value: result,
        unknown_completion_before_return,
        dependency_coverage,
        caller_continuation_open: true,
    }
}

fn observe_fallthrough(
    context: &ObservationContext<'_>,
    site: &NodeId,
    state: &BranchState,
    path_index: u32,
) -> ResultObservation {
    let ObservationContext {
        procedure,
        paths,
        function_span,
        ..
    } = *context;
    let mut walker = DependencyWalker {
        procedure,
        paths,
        decisions: &state.decisions,
        visiting: BTreeSet::new(),
    };
    let guards = walker.guards(state);
    let unknown_completion_before_return = unknown_before(procedure, site, &state.decisions);
    let span = SourceSpan {
        path: function_span.path.clone(),
        byte_start: function_span.byte_end,
        byte_end: function_span.byte_end,
        start_line: function_span.end_line,
        end_line: function_span.end_line,
    };
    ResultObservation {
        site: site.clone(),
        path_index,
        kind: ResultExitKind::Fallthrough,
        span,
        effective_condition: condition(state),
        guards,
        value: ResultValue::Undefined,
        unknown_completion_before_return,
        dependency_coverage: if unknown_completion_before_return {
            Coverage::Partial
        } else {
            Coverage::Complete
        },
        caller_continuation_open: true,
    }
}

fn value_unresolved(value: &ResultValue) -> bool {
    match value {
        ResultValue::Undefined => false,
        ResultValue::Expression { dependency, .. } => dependency.unresolved,
    }
}

fn unknown_before(
    procedure: &ProcedureIr,
    target: &NodeId,
    decisions: &BTreeMap<NodeId, bool>,
) -> bool {
    let mut queue = VecDeque::from([(procedure.entry.clone(), false)]);
    let mut seen = BTreeSet::new();
    while let Some((id, unknown)) = queue.pop_front() {
        if !seen.insert((id.clone(), unknown)) {
            continue;
        }
        if &id == target {
            if unknown
                || matches!(
                    procedure.nodes[&id].operation,
                    Operation::UnknownEffect { .. } | Operation::Call { .. }
                )
            {
                return true;
            }
            continue;
        }
        let next_unknown = unknown
            || matches!(
                procedure.nodes[&id].operation,
                Operation::UnknownEffect { .. } | Operation::Call { .. }
            );
        for edge in procedure.edges.iter().filter(|edge| edge.source == id) {
            if let EdgeKind::Branch { outcome } = edge.kind
                && decisions.get(&id) != Some(&outcome)
            {
                continue;
            }
            if matches!(edge.kind, EdgeKind::Normal | EdgeKind::Branch { .. }) {
                queue.push_back((edge.target.clone(), next_unknown));
            }
        }
    }
    false
}

struct DependencyWalker<'a> {
    procedure: &'a ProcedureIr,
    paths: &'a BranchPaths,
    decisions: &'a BTreeMap<NodeId, bool>,
    visiting: BTreeSet<(Place, NodeId)>,
}

impl DependencyWalker<'_> {
    fn guards(&mut self, state: &BranchState) -> Vec<ResultGuard> {
        state
            .decisions
            .iter()
            .filter_map(|(branch, outcome)| {
                let Operation::Branch { condition } = &self.procedure.nodes[branch].operation
                else {
                    return None;
                };
                let condition = self.trace(condition, branch, None);
                Some(ResultGuard {
                    branch: branch.clone(),
                    span: self.procedure.nodes[branch].span.clone(),
                    outcome: *outcome,
                    condition,
                })
            })
            .collect()
    }

    fn compatible_facts(&self, node: &NodeId) -> Vec<&BTreeSet<Fact>> {
        self.paths
            .at
            .get(node)
            .into_iter()
            .flatten()
            .filter(|state| {
                state
                    .decisions
                    .iter()
                    .all(|(branch, outcome)| self.decisions.get(branch) == Some(outcome))
            })
            .map(|state| &state.facts)
            .collect()
    }

    fn trace(
        &mut self,
        place: &Place,
        at: &NodeId,
        role: Option<ComputeInputRole>,
    ) -> ResultDependency {
        let key = (place.clone(), at.clone());
        if !self.visiting.insert(key.clone()) {
            return ResultDependency {
                node: at.clone(),
                span: self
                    .procedure
                    .nodes
                    .get(at)
                    .and_then(|node| node.span.clone()),
                role,
                operation: "cycle".into(),
                literal: None,
                operator: None,
                origin: None,
                inputs: Vec::new(),
                unresolved: true,
            };
        }
        let mut output = match place {
            Place::Temporary(id) => self.trace_temporary(id, role),
            Place::Binding(binding) => self.trace_binding(binding, at, role),
        };
        output.unresolved |= output.inputs.iter().any(|input| input.unresolved);
        self.visiting.remove(&key);
        output
    }

    fn trace_temporary(&mut self, id: &NodeId, role: Option<ComputeInputRole>) -> ResultDependency {
        let node = &self.procedure.nodes[id];
        let (operation, inputs, origin, unresolved) = match &node.operation {
            Operation::Literal { definition, .. } => (
                "literal".into(),
                Vec::new(),
                Some(Source::Write(definition.clone())),
                false,
            ),
            Operation::Read { source, .. } => (
                "read".into(),
                vec![self.trace(source, id, None)],
                None,
                false,
            ),
            Operation::Compute {
                inputs, operator, ..
            } => {
                let selected: Vec<_> = if let PrimitiveOperator::ValueJoin { branch } = operator {
                    self.decisions
                        .get(branch)
                        .and_then(|outcome| inputs.get(if *outcome { 0 } else { 1 }))
                        .into_iter()
                        .collect()
                } else {
                    inputs.iter().collect()
                };
                let dependencies = selected
                    .into_iter()
                    .map(|input| self.trace(&input.place, id, Some(input.role.clone())))
                    .collect();
                (format!("compute:{operator:?}"), dependencies, None, false)
            }
            Operation::UnknownEffect {
                inputs,
                description,
                ..
            } => (
                format!("unknown_effect:{description}"),
                inputs
                    .iter()
                    .map(|input| self.trace(&input.place, id, Some(input.role.clone())))
                    .collect(),
                None,
                true,
            ),
            Operation::Call { arguments, .. } => (
                "unresolved_call".into(),
                arguments
                    .iter()
                    .map(|place| self.trace(place, id, None))
                    .collect(),
                None,
                true,
            ),
            _ => ("missing_temporary_producer".into(), Vec::new(), None, true),
        };
        ResultDependency {
            node: id.clone(),
            span: node.span.clone(),
            role,
            operation,
            literal: match &node.operation {
                Operation::Literal {
                    literal_kind,
                    raw,
                    cooked,
                    ..
                } => match literal_kind {
                    super::ir::LiteralKind::Boolean => Some(FunctionKnownValue::Boolean {
                        value: raw == "true",
                    }),
                    super::ir::LiteralKind::Number => {
                        Some(FunctionKnownValue::Number { value: raw.clone() })
                    }
                    super::ir::LiteralKind::String => cooked
                        .clone()
                        .or_else(|| {
                            let inner = raw.strip_prefix('\'')?.strip_suffix('\'')?;
                            (!inner.contains('\\')).then(|| inner.to_owned())
                        })
                        .map(|value| FunctionKnownValue::String { value }),
                    super::ir::LiteralKind::Null => Some(FunctionKnownValue::Null),
                    super::ir::LiteralKind::Undefined => Some(FunctionKnownValue::Undefined),
                    _ => None,
                },
                _ => None,
            },
            operator: match &node.operation {
                Operation::Compute { operator, .. } => Some(operator.clone()),
                _ => None,
            },
            origin,
            inputs,
            unresolved,
        }
    }

    fn trace_binding(
        &mut self,
        binding: &BindingId,
        at: &NodeId,
        role: Option<ComputeInputRole>,
    ) -> ResultDependency {
        let place = Place::Binding(binding.clone());
        let mut writes = BTreeSet::new();
        for facts in self.compatible_facts(at) {
            for fact in facts {
                if let Fact::LastWrite {
                    place: fact_place,
                    write,
                } = fact
                    && fact_place == &place
                {
                    writes.insert(write.clone());
                }
            }
        }
        let mut inputs = Vec::new();
        for write in writes {
            if let Some(node) = self.procedure.nodes.values().find(|node|
                matches!(&node.operation, Operation::Write { definition, .. } if definition == &write)) {
                if let Operation::Write { sources, .. } = &node.operation {
                    let writer_inputs = sources.iter()
                        .map(|source| self.trace(source, &node.id, None)).collect();
                    inputs.push(ResultDependency {
                        node: node.id.clone(), span: node.span.clone(), role: None, operation: "write".into(),
                        literal: None, operator: None,
                        origin: Some(Source::Write(write.clone())), inputs: writer_inputs,
                        unresolved: false,
                    });
                }
            } else if self.procedure.parameters.iter().any(|parameter|
                parameter.binding == *binding && parameter.entry_definition == write) {
                inputs.push(ResultDependency { node: self.procedure.entry.clone(), span: self.procedure.nodes[&self.procedure.entry].span.clone(), role: None,
                    operation: "positional_input".into(), origin: Some(Source::FunctionInput(binding.clone())),
                    literal: None, operator: None,
                    inputs: Vec::new(), unresolved: false });
            }
        }
        if inputs.is_empty()
            && self
                .procedure
                .parameters
                .iter()
                .any(|parameter| parameter.binding == *binding)
        {
            // An unsupported effect can erase the last-write fact. Preserve the
            // syntactic parameter dependency without claiming its value survived.
            inputs.push(ResultDependency {
                node: self.procedure.entry.clone(),
                span: self.procedure.nodes[&self.procedure.entry].span.clone(),
                role: None,
                operation: "possible_positional_input".into(),
                literal: None,
                operator: None,
                origin: Some(Source::FunctionInput(binding.clone())),
                inputs: Vec::new(),
                unresolved: true,
            });
        }
        let unresolved = inputs.is_empty();
        ResultDependency {
            node: at.clone(),
            span: self
                .procedure
                .nodes
                .get(at)
                .and_then(|node| node.span.clone()),
            role,
            operation: "binding_value".into(),
            literal: None,
            operator: None,
            origin: None,
            inputs,
            unresolved,
        }
    }
}

#[cfg(test)]
#[path = "function_results_tests.rs"]
mod tests;
