//! Public, snapshot-backed orchestration and evidence-based report rendering.

use super::adapters::typescript::{
    TypeScriptBindingIndex, alignable_bindings, index_bindings, lower_containing_procedure,
};
use super::compare::{AlignmentError, AlignmentResult, align_procedures};
use super::comparison::{ComparisonError, ComparisonInput, compare_flow_slices};
use super::model::*;
use super::provenance::seed_entry_facts;
use super::slicing::{SliceError, SliceOutput, SliceRequest, build_flow_slice};
use super::snapshots::{AnalysisEnvironment, SnapshotProvider, validate_query};
use super::solver::{
    IntraproceduralSolver, Solver, SolverError, SolverRequest, SystemSolverControl,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug)]
pub enum AnalysisError {
    Input(InputError),
    Adapter(String),
    Alignment(AlignmentError),
    Solver(SolverError),
    Slice(SliceError),
    Comparison(ComparisonError),
    Schema(SchemaError),
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "variable-flow analysis failed: {self:?}")
    }
}

impl std::error::Error for AnalysisError {}

impl From<InputError> for AnalysisError {
    fn from(value: InputError) -> Self {
        Self::Input(value)
    }
}
impl From<AlignmentError> for AnalysisError {
    fn from(value: AlignmentError) -> Self {
        Self::Alignment(value)
    }
}
impl From<SolverError> for AnalysisError {
    fn from(value: SolverError) -> Self {
        Self::Solver(value)
    }
}
impl From<SliceError> for AnalysisError {
    fn from(value: SliceError) -> Self {
        Self::Slice(value)
    }
}
impl From<ComparisonError> for AnalysisError {
    fn from(value: ComparisonError) -> Self {
        Self::Comparison(value)
    }
}
impl From<SchemaError> for AnalysisError {
    fn from(value: SchemaError) -> Self {
        Self::Schema(value)
    }
}

fn read_index<P: SnapshotProvider>(
    provider: &P,
    handle: &SnapshotHandle,
    path: &str,
) -> Result<(String, TypeScriptBindingIndex), AnalysisError> {
    let bytes = provider.read_file(handle, path)?;
    let source = String::from_utf8(bytes)
        .map_err(|error| AnalysisError::Adapter(format!("{path}: invalid UTF-8: {error}")))?;
    let index = index_bindings(handle.id.clone(), path, &source)
        .map_err(|error| AnalysisError::Adapter(error.to_string()))?;
    Ok((source, index))
}

fn opposite_path(query: &VariableFlowQuery, path: &str, side: SnapshotSide) -> String {
    query
        .diff
        .changes
        .iter()
        .find_map(|change| match side {
            SnapshotSide::Before if change.before_path.as_deref() == Some(path) => {
                change.after_path.clone()
            }
            SnapshotSide::After if change.after_path.as_deref() == Some(path) => {
                change.before_path.clone()
            }
            _ => None,
        })
        .unwrap_or_else(|| path.to_owned())
}

fn inferred_counterpart<'a>(
    index: &'a TypeScriptBindingIndex,
    selected: &super::adapters::typescript::LexicalBinding,
) -> Result<&'a super::adapters::typescript::LexicalBinding, AnalysisError> {
    let mut matches = index.bindings.iter().filter(|binding| {
        binding.name == selected.name
            && binding.enclosing_symbol == selected.enclosing_symbol
            && binding.kind == selected.kind
    });
    let first = matches.next().ok_or_else(|| {
        InputError::InvalidSelector(
            "selected binding has no supported counterpart in the other snapshot".into(),
        )
    })?;
    if matches.next().is_some() {
        return Err(InputError::InvalidSelector(
            "selected binding counterpart is ambiguous; provide an explicit selector".into(),
        )
        .into());
    }
    Ok(first)
}

/// Analyze one lexical variable across a validated pair of immutable snapshots.
pub fn analyze_variable_flow<P: SnapshotProvider>(
    query: VariableFlowQuery,
    provider: &P,
    before_environment: &AnalysisEnvironment,
    after_environment: &AnalysisEnvironment,
) -> Result<VariableFlowReport, AnalysisError> {
    validate_query(&query, provider, before_environment, after_environment)?;
    if !matches!(query.entry, EntryPoint::ContainingFunction) {
        return Err(InputError::CapabilityMismatch(
            "call-entry analysis is not supported in stage 1".into(),
        )
        .into());
    }
    let selected_side = query.selected_binding.snapshot.side;
    let selected_path = &query.selected_binding.declaration.path;
    let other_path = query
        .counterpart
        .as_ref()
        .map(|selector| selector.declaration.path.clone())
        .unwrap_or_else(|| opposite_path(&query, selected_path, selected_side));
    let (before_path, after_path) = if selected_side == SnapshotSide::Before {
        (selected_path.clone(), other_path)
    } else {
        (other_path, selected_path.clone())
    };
    let (before_source, before_index) = read_index(provider, &query.before, &before_path)?;
    let (after_source, after_index) = read_index(provider, &query.after, &after_path)?;
    let (before_binding, after_binding) = if selected_side == SnapshotSide::Before {
        let selected = before_index.resolve_selector(&query.selected_binding)?;
        let counterpart = if let Some(selector) = &query.counterpart {
            after_index.resolve_selector(selector)?
        } else {
            inferred_counterpart(&after_index, selected)?
        };
        (selected.id.clone(), counterpart.id.clone())
    } else {
        let selected = after_index.resolve_selector(&query.selected_binding)?;
        let counterpart = if let Some(selector) = &query.counterpart {
            before_index.resolve_selector(selector)?
        } else {
            inferred_counterpart(&before_index, selected)?
        };
        (counterpart.id.clone(), selected.id.clone())
    };
    let before = lower_containing_procedure(&before_source, &before_index, &before_binding)
        .map_err(|error| AnalysisError::Adapter(error.to_string()))?;
    let after = lower_containing_procedure(&after_source, &after_index, &after_binding)
        .map_err(|error| AnalysisError::Adapter(error.to_string()))?;
    let before_alignable = alignable_bindings(&before_index, &before.procedure);
    let after_alignable = alignable_bindings(&after_index, &after.procedure);
    let selected_id = if selected_side == SnapshotSide::Before {
        &before_binding
    } else {
        &after_binding
    };
    let explicit = query.counterpart.as_ref().map(|_| {
        if selected_side == SnapshotSide::Before {
            &after_binding
        } else {
            &before_binding
        }
    });
    let alignment = align_procedures(
        &before.procedure,
        &after.procedure,
        &before_alignable,
        &after_alignable,
        &query.diff,
        selected_id,
        explicit,
    )?;
    let solver = IntraproceduralSolver::new();
    let control = SystemSolverControl::new();
    let before_solved = solver.solve(
        SolverRequest {
            procedure: &before.procedure,
            entry_facts: seed_entry_facts(&before.procedure),
            limits: query.limits.clone(),
        },
        &control,
    )?;
    let after_solved = solver.solve(
        SolverRequest {
            procedure: &after.procedure,
            entry_facts: seed_entry_facts(&after.procedure),
            limits: query.limits.clone(),
        },
        &control,
    )?;
    let mut before_slice = build_flow_slice(SliceRequest {
        procedure: &before.procedure,
        solved: &before_solved,
        selected_binding: &before_binding,
        declared_scope: &scope_name(&before_index, &before_binding),
    })?;
    let mut after_slice = build_flow_slice(SliceRequest {
        procedure: &after.procedure,
        solved: &after_solved,
        selected_binding: &after_binding,
        declared_scope: &scope_name(&after_index, &after_binding),
    })?;
    annotate_operations(&mut before_slice.graph, &before_source);
    annotate_operations(&mut after_slice.graph, &after_source);
    let query_key = serde_json::to_string(&query.selected_binding)
        .map_err(|error| AnalysisError::Adapter(error.to_string()))?;
    let comparison = compare_flow_slices(ComparisonInput {
        before: &before_slice,
        after: &after_slice,
        before_ir: &before.procedure,
        after_ir: &after.procedure,
        before_binding: Some(&before_binding),
        after_binding: Some(&after_binding),
        alignment: &alignment,
        query_key: &query_key,
        before_presence: &BTreeMap::new(),
        after_presence: &BTreeMap::new(),
        condition_proofs: &BTreeMap::new(),
    })?;
    assemble_report(
        query,
        before_binding,
        after_binding,
        alignment,
        before_slice,
        after_slice,
        before_solved,
        after_solved,
        before.diagnostics,
        after.diagnostics,
        comparison.deltas,
        &before.procedure,
        &after.procedure,
        &before_source,
        &after_source,
    )
}

fn annotate_operations(graph: &mut FlowGraph, source: &str) {
    graph.nodes = graph
        .nodes
        .iter()
        .cloned()
        .map(|mut node| {
            let start = node.span.byte_start as usize;
            let end = node.span.byte_end as usize;
            if let Some(snippet) = source.get(start..end) {
                let snippet = snippet.trim();
                if !snippet.is_empty() {
                    node.operation = format!("{} `{snippet}`", node.operation);
                }
            }
            node
        })
        .collect();
}

fn scope_name(index: &TypeScriptBindingIndex, binding: &BindingId) -> String {
    let owner = index
        .bindings
        .iter()
        .find(|candidate| &candidate.id == binding)
        .and_then(|candidate| candidate.enclosing_symbol.as_deref());
    owner
        .map(|owner| format!("function {owner}"))
        .unwrap_or_else(|| "module".into())
}

#[allow(clippy::too_many_arguments)]
fn assemble_report(
    query: VariableFlowQuery,
    before_binding: BindingId,
    after_binding: BindingId,
    alignment: AlignmentResult,
    before: SliceOutput,
    after: SliceOutput,
    before_solved: super::solver::SolverOutput,
    after_solved: super::solver::SolverOutput,
    before_parse_diagnostics: Vec<Diagnostic>,
    after_parse_diagnostics: Vec<Diagnostic>,
    deltas: BTreeSet<DeltaRecord>,
    before_ir: &super::ir::ProcedureIr,
    after_ir: &super::ir::ProcedureIr,
    before_source: &str,
    after_source: &str,
) -> Result<VariableFlowReport, AnalysisError> {
    let mut diagnostics: BTreeSet<_> = before
        .diagnostics
        .iter()
        .chain(&after.diagnostics)
        .chain(&alignment.diagnostics)
        .chain(&before_parse_diagnostics)
        .chain(&after_parse_diagnostics)
        .cloned()
        .collect();
    diagnostics.extend(
        before_solved
            .diagnostics
            .iter()
            .chain(&after_solved.diagnostics)
            .cloned(),
    );
    let mut unknown_frontiers: BTreeSet<_> = before
        .unknown_frontiers
        .iter()
        .chain(&after.unknown_frontiers)
        .cloned()
        .collect();
    for diagnostic in &diagnostics {
        if let Some(node) = &diagnostic.frontier {
            for direction in [Direction::Upstream, Direction::Downstream] {
                let name = match direction {
                    Direction::Upstream => "upstream",
                    Direction::Downstream => "downstream",
                };
                if diagnostic.affected_flows.contains(name) {
                    unknown_frontiers.insert(UnknownFrontier {
                        node: node.clone(),
                        next_operation: diagnostic.message.clone(),
                        diagnostic: diagnostic.code.clone(),
                        affected_direction: direction,
                    });
                }
            }
        }
    }
    let offset = before
        .witnesses
        .iter()
        .map(|witness| witness.id.0)
        .max()
        .map_or(0, |id| id + 1);
    let witnesses = before
        .witnesses
        .iter()
        .cloned()
        .chain(after.witnesses.iter().cloned().map(|mut witness| {
            witness.id.0 += offset;
            witness
        }))
        .collect();
    let path_endings = before
        .path_endings
        .iter()
        .cloned()
        .chain(after.path_endings.iter().cloned().map(|mut ending| {
            ending.witness.0 += offset;
            ending
        }))
        .collect();
    let flow_extent = FlowExtent {
        before: before.extent.clone(),
        after: after.extent.clone(),
    };
    let completeness = match (before.completeness, after.completeness) {
        (Completeness::Unsupported, _) | (_, Completeness::Unsupported) => {
            Completeness::Unsupported
        }
        (Completeness::Partial, _) | (_, Completeness::Partial) => Completeness::Partial,
        _ => Completeness::CompleteForQuery,
    };
    let mut region_completeness = BTreeMap::new();
    for (name, extent) in [("before", &before.extent), ("after", &after.extent)] {
        region_completeness.insert(format!("{name}:upstream"), extent.upstream);
        region_completeness.insert(format!("{name}:downstream"), extent.downstream);
    }
    for (side, solved) in [("before", &before_solved), ("after", &after_solved)] {
        for (region, coverage) in &solved.region_completeness {
            let direction = match region.direction {
                Direction::Upstream => "upstream",
                Direction::Downstream => "downstream",
            };
            region_completeness.insert(format!("{side}:{}:{direction}", region.name), *coverage);
        }
    }
    let mut stats = before_solved.stats.clone();
    stats.elapsed_ms += after_solved.stats.elapsed_ms;
    stats.peak_memory_bytes = stats
        .peak_memory_bytes
        .max(after_solved.stats.peak_memory_bytes);
    stats.processed_path_edges += after_solved.stats.processed_path_edges;
    stats.graph_nodes = (before.graph.nodes.len() + after.graph.nodes.len()) as u64;
    stats.graph_edges = (before.graph.edges.len() + after.graph.edges.len()) as u64;
    stats.witnesses_emitted = before.witnesses.len() as u64 + after.witnesses.len() as u64;
    stats.witnesses_omitted += after_solved.stats.witnesses_omitted;
    stats.limits_hit.extend(after_solved.stats.limits_hit);
    stats.graph_truncated |= after_solved.stats.graph_truncated;
    stats.witnesses_truncated |= after_solved.stats.witnesses_truncated;
    let human_summary = render_human_summary(
        &deltas,
        &before.graph,
        &after.graph,
        &alignment.nodes,
        &diagnostics,
        &unknown_frontiers,
        before_ir,
        after_ir,
        before_source,
        after_source,
        &before_binding,
        &after_binding,
    );
    let entry_assumptions = before_ir
        .parameters
        .iter()
        .chain(&after_ir.parameters)
        .map(|parameter| EntryAssumption {
            name: format!("parameter {}", parameter.index),
            value: None,
            source: Source::FunctionInput(parameter.binding.clone()),
        })
        .collect();
    let report = VariableFlowReport {
        schema_version: REPORT_SCHEMA_VERSION,
        analysis_version: env!("CARGO_PKG_VERSION").into(),
        snapshots: BTreeSet::from([query.before.id.clone(), query.after.id.clone()]),
        query: ResolvedQuery {
            request: query.clone(),
            before_binding: Some(before_binding.clone()),
            after_binding: Some(after_binding.clone()),
        },
        entry_assumptions,
        before_graph: before.graph,
        after_graph: after.graph,
        alignment: alignment.nodes,
        deltas,
        human_summary,
        witnesses,
        diagnostics,
        unknown_frontiers,
        capabilities: query.capabilities,
        summaries_used: BTreeSet::new(),
        flow_extent,
        path_endings,
        completeness,
        region_completeness,
        stats,
    };
    report.validate()?;
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
fn render_human_summary(
    deltas: &BTreeSet<DeltaRecord>,
    before: &FlowGraph,
    after: &FlowGraph,
    alignment: &BTreeSet<Alignment>,
    diagnostics: &BTreeSet<Diagnostic>,
    frontiers: &BTreeSet<UnknownFrontier>,
    before_ir: &super::ir::ProcedureIr,
    after_ir: &super::ir::ProcedureIr,
    before_source: &str,
    after_source: &str,
    before_binding: &BindingId,
    after_binding: &BindingId,
) -> String {
    let before_nodes: BTreeMap<_, _> = before.nodes.iter().map(|node| (&node.id, node)).collect();
    let after_nodes: BTreeMap<_, _> = after.nodes.iter().map(|node| (&node.id, node)).collect();
    let by_logical: BTreeMap<_, _> = alignment.iter().map(|item| (item.logical, item)).collect();
    let label = |logical: &LogicalNodeId, side: SnapshotSide| -> String {
        by_logical
            .get(logical)
            .and_then(|item| {
                let id = match side {
                    SnapshotSide::Before => item.before.as_ref(),
                    SnapshotSide::After => item.after.as_ref(),
                }?;
                let graph_node = match side {
                    SnapshotSide::Before => before_nodes.get(id),
                    SnapshotSide::After => after_nodes.get(id),
                };
                graph_node
                    .map(|node| {
                        format!(
                            "{} at {}:{}",
                            node.operation, node.span.path, node.span.start_line
                        )
                    })
                    .or_else(|| {
                        let (ir, source) = match side {
                            SnapshotSide::Before => (before_ir, before_source),
                            SnapshotSide::After => (after_ir, after_source),
                        };
                        let node = ir.nodes.get(id)?;
                        let span = node.span.as_ref()?;
                        let snippet = source
                            .get(span.byte_start as usize..span.byte_end as usize)?
                            .trim();
                        let kind = match node.operation {
                            super::ir::Operation::Write { .. } => "write",
                            super::ir::Operation::Return { .. } => "return",
                            super::ir::Operation::Literal { .. } => "literal",
                            _ => "operation",
                        };
                        Some(format!(
                            "{kind} `{snippet}` at {}:{}",
                            span.path, span.start_line
                        ))
                    })
            })
            .unwrap_or_else(|| format!("operation {}", logical.0))
    };
    let mut parts = BTreeSet::new();
    for record in deltas {
        match &record.delta {
            FlowDelta::WriteAdded { after: id } => {
                if let Some(node) = after_nodes.get(id) {
                    parts.insert(format!(
                        "Added selected-binding {} at {}:{}.",
                        node.operation, node.span.path, node.span.start_line
                    ));
                }
            }
            FlowDelta::WriteRemoved { before: id } => {
                if let Some(node) = before_nodes.get(id) {
                    parts.insert(format!(
                        "Removed selected-binding {} at {}:{}.",
                        node.operation, node.span.path, node.span.start_line
                    ));
                }
            }
            FlowDelta::OperationChanged { logical, .. } => {
                parts.insert(format!(
                    "Changed operation {} to {}.",
                    label(logical, SnapshotSide::Before),
                    label(logical, SnapshotSide::After)
                ));
                if let Some(item) = by_logical.get(logical)
                    && item
                        .before
                        .as_ref()
                        .and_then(|id| before_ir.nodes.get(id))
                        .is_some_and(|node| {
                            matches!(&node.operation,
                            super::ir::Operation::Write { target: Place::Binding(binding), .. }
                            if binding == before_binding)
                        })
                    && item
                        .after
                        .as_ref()
                        .and_then(|id| after_ir.nodes.get(id))
                        .is_some_and(|node| {
                            matches!(&node.operation,
                            super::ir::Operation::Write { target: Place::Binding(binding), .. }
                            if binding == after_binding)
                        })
                {
                    parts.insert("Its selected-binding write was retained.".into());
                }
            }
            FlowDelta::SliceMembershipChanged {
                logical,
                change: MembershipChange::Left,
            } => {
                let operation = label(logical, SnapshotSide::Before);
                if operation.starts_with("return ") {
                    parts.insert(format!("Selected binding no longer reaches {operation}."));
                } else {
                    parts.insert(format!(
                        "{operation} left the selected binding's flow slice."
                    ));
                }
            }
            FlowDelta::ValueSourceChanged {
                consumer,
                projection,
                before_sources,
                after_sources,
                ..
            } => {
                let sources = |items: &BTreeSet<LogicalNodeId>, side| {
                    items
                        .iter()
                        .map(|id| label(id, side))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let relocated_from = deltas.iter().find_map(|candidate| {
                    let FlowDelta::ValueSourceChanged {
                        consumer: parent_consumer,
                        projection: parent_projection,
                        before_sources: parent_before,
                        after_sources: parent_after,
                        ..
                    } = &candidate.delta
                    else {
                        return None;
                    };
                    (parent_consumer == consumer
                        && projection
                            .strip_prefix(parent_projection)
                            .is_some_and(|suffix| suffix.starts_with('.'))
                        && !parent_before.is_empty()
                        && parent_after.is_empty()
                        && before_sources.is_empty()
                        && !after_sources.is_empty())
                    .then_some((parent_projection, parent_before))
                });
                if let Some((parent_projection, parent_sources)) = relocated_from {
                    parts.insert(format!(
                        "Consumer input {} moved sources from ({parent_projection}) [{}] to ({projection}) [{}].",
                        label(consumer, SnapshotSide::After),
                        sources(parent_sources, SnapshotSide::Before),
                        sources(after_sources, SnapshotSide::After)
                    ));
                    continue;
                }
                let relocates_to_child = after_sources.is_empty()
                    && deltas.iter().any(|candidate| {
                        matches!(
                            &candidate.delta,
                            FlowDelta::ValueSourceChanged {
                                consumer: child_consumer,
                                projection: child_projection,
                                before_sources: child_before,
                                after_sources: child_after,
                                ..
                            } if child_consumer == consumer
                                && child_projection
                                    .strip_prefix(projection)
                                    .is_some_and(|suffix| suffix.starts_with('.'))
                                && child_before.is_empty()
                                && !child_after.is_empty()
                        )
                    });
                if !relocates_to_child {
                    parts.insert(format!(
                        "Consumer input {} ({projection}) changed sources from [{}] to [{}].",
                        label(consumer, SnapshotSide::After),
                        sources(before_sources, SnapshotSide::Before),
                        sources(after_sources, SnapshotSide::After)
                    ));
                }
                for source in before_sources.difference(after_sources) {
                    if let Some(item) = by_logical.get(source)
                        && item.after.is_some()
                        && item
                            .before
                            .as_ref()
                            .and_then(|id| before_nodes.get(id))
                            .is_some_and(|node| node.operation.starts_with("write"))
                    {
                        let selected_writer = item
                            .before
                            .as_ref()
                            .and_then(|id| before_ir.nodes.get(id))
                            .is_some_and(|node| {
                                matches!(&node.operation,
                                super::ir::Operation::Write { target: Place::Binding(binding), .. }
                                if binding == before_binding)
                            });
                        let effect = if selected_writer {
                            "no longer reaches this input"
                        } else {
                            "no longer carries the selected binding's value to this input"
                        };
                        parts.insert(format!(
                            "Earlier writer {} remains in the code but {effect}.",
                            label(source, SnapshotSide::Before).replacen("write ", "", 1)
                        ));
                    }
                }
            }
            FlowDelta::FlowConditionChanged {
                source,
                target,
                before,
                after,
                ..
            } => {
                parts.insert(format!(
                    "Flow from {} to {} changed condition from {before} to {after}.",
                    label(source, SnapshotSide::After),
                    label(target, SnapshotSide::After)
                ));
            }
            _ => {}
        }
    }
    for frontier in frontiers {
        let location = before_nodes
            .get(&frontier.node)
            .or_else(|| after_nodes.get(&frontier.node))
            .map(|node| format!("{}:{}", node.span.path, node.span.start_line))
            .unwrap_or_else(|| format!("{:?}", frontier.node));
        parts.insert(format!(
            "Unknown {:?} boundary at {}: {}.",
            frontier.affected_direction, location, frontier.next_operation
        ));
    }
    for diagnostic in diagnostics {
        if diagnostic.frontier.is_none()
            && matches!(
                diagnostic.code,
                DiagnosticCode::AmbiguousMatch
                    | DiagnosticCode::ParseError
                    | DiagnosticCode::OutputTruncated
                    | DiagnosticCode::AnalysisBudgetExceeded
            )
        {
            let prefix = if diagnostic.code == DiagnosticCode::AmbiguousMatch {
                "Alignment uncertain"
            } else {
                "Unknown boundary"
            };
            parts.insert(format!("{prefix}: {}.", diagnostic.message));
        }
    }
    if parts.is_empty() {
        "No established flow changes within the analyzed scope.".into()
    } else {
        parts.into_iter().collect::<Vec<_>>().join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ifds::snapshots::{InMemorySnapshot, InMemorySnapshotProvider};

    fn selector(snapshot: SnapshotId, source: &str, name: &str) -> BindingSelector {
        let needle = format!("let {name}");
        let start = source.find(&needle).unwrap() + 4;
        let line = 1 + source[..start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count() as u32;
        BindingSelector {
            snapshot,
            declaration: SourceSpan {
                path: "main.ts".into(),
                byte_start: start as u64,
                byte_end: (start + name.len()) as u64,
                start_line: line,
                end_line: line,
            },
            expected_name: Some(name.into()),
            expected_enclosing_symbol: None,
        }
    }

    fn run(before: &str, after: &str, name: &str) -> VariableFlowReport {
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
            repository_id: "repo".into(),
        };
        let after_handle = SnapshotHandle {
            id: after_id.clone(),
            repository_id: "repo".into(),
        };
        let provider = InMemorySnapshotProvider::new([
            InMemorySnapshot {
                handle: before_handle.clone(),
                files: BTreeMap::from([(
                    "main.ts".into(),
                    ("blob-before".into(), before.as_bytes().to_vec()),
                )]),
            },
            InMemorySnapshot {
                handle: after_handle.clone(),
                files: BTreeMap::from([(
                    "main.ts".into(),
                    ("blob-after".into(), after.as_bytes().to_vec()),
                )]),
            },
        ]);
        let capabilities = CapabilitySet {
            stage: 1,
            capabilities: BTreeSet::from(["straight_line".into()]),
            version: "stage-1".into(),
        };
        let environment = AnalysisEnvironment {
            capabilities: capabilities.clone(),
            summaries: BTreeSet::new(),
        };
        let query = VariableFlowQuery {
            before: before_handle,
            after: after_handle,
            diff: RepositoryDiff {
                before_content_id: before_id.content_id.clone(),
                after_content_id: after_id.content_id.clone(),
                changes: BTreeSet::from([FileChange {
                    kind: FileChangeKind::Modified,
                    before_path: Some("main.ts".into()),
                    after_path: Some("main.ts".into()),
                    before_content_id: Some("blob-before".into()),
                    after_content_id: Some("blob-after".into()),
                }]),
            },
            selected_binding: selector(before_id, before, name),
            counterpart: Some(selector(after_id, after, name)),
            entry: EntryPoint::ContainingFunction,
            capabilities,
            summaries: BTreeSet::new(),
            limits: AnalysisLimits {
                time_ms: 1000,
                memory_bytes: 64_000_000,
                processed_path_edges: 100_000,
                witnesses_per_relation: 3,
                output_nodes: 100_000,
            },
        };
        analyze_variable_flow(query, &provider, &environment, &environment).unwrap()
    }

    const OVERWRITE_BEFORE: &str = "function example() {\n  let x = 1;\n  return x;\n}\n";
    const OVERWRITE_AFTER: &str = "function example() {\n  let x = 1;\n  x = 2;\n  return x;\n}\n";

    #[test]
    fn ifds_k013_straight_line_api() {
        let initializer = run(
            OVERWRITE_BEFORE,
            "function example() {\n  let x = 2;\n  return x;\n}\n",
            "x",
        );
        assert!(
            initializer
                .deltas
                .iter()
                .any(|record| matches!(record.delta, FlowDelta::OperationChanged { .. }))
        );
        assert!(
            initializer
                .deltas
                .iter()
                .any(|record| matches!(record.delta, FlowDelta::ValueSourceChanged { .. }))
        );
        assert!(!initializer.deltas.iter().any(|record| matches!(
            record.delta,
            FlowDelta::WriteAdded { .. } | FlowDelta::WriteRemoved { .. }
        )));
        let report = run(OVERWRITE_BEFORE, OVERWRITE_AFTER, "x");
        assert!(
            report
                .deltas
                .iter()
                .any(|record| matches!(record.delta, FlowDelta::WriteAdded { .. }))
        );
        assert!(report.deltas.iter().any(|record| matches!(
            record.delta,
            FlowDelta::FlowRemoved {
                relation: RelationKind::Reaches,
                ..
            }
        )));
        assert!(report.deltas.iter().any(|record| matches!(
            record.delta,
            FlowDelta::FlowAdded {
                relation: RelationKind::Reaches,
                ..
            }
        )));
        assert!(
            !report
                .deltas
                .iter()
                .any(|record| matches!(record.delta, FlowDelta::WriteRemoved { .. }))
        );
        assert_eq!(report.completeness, Completeness::CompleteForQuery);
    }

    #[test]
    fn ifds_k013_full_downstream_api() {
        let report = run(
            "function chain(input: number) {\n  let x = input;\n  const y = x + 1;\n  return y * 2;\n}\n",
            "function chain(input: number) {\n  let x = input + 1;\n  const y = x + 1;\n  return y * 2;\n}\n",
            "x",
        );
        assert!(report.before_graph.nodes.len() >= 3);
        assert!(report.after_graph.nodes.len() >= 3);
        assert!(!report.flow_extent.after.value_lifecycle_closed);
    }

    #[test]
    fn ifds_k013_report_round_trip() {
        let report = run(OVERWRITE_BEFORE, OVERWRITE_AFTER, "x");
        assert_eq!(
            VariableFlowReport::from_json(&report.to_canonical_json().unwrap()).unwrap(),
            report
        );
    }

    #[test]
    fn ifds_k013_deterministic_groups() {
        let left = run(OVERWRITE_BEFORE, OVERWRITE_AFTER, "x");
        let right = run(OVERWRITE_BEFORE, OVERWRITE_AFTER, "x");
        assert_eq!(left.deltas, right.deltas);
    }

    #[test]
    fn ifds_k013_partial_is_visible() {
        let report = run(
            "function example() {\n  let x = 1;\n  x = mystery(x);\n  return x;\n}\n",
            "function example() {\n  let x = 1;\n  x = mystery(x);\n  return x;\n}\n",
            "x",
        );
        assert_ne!(report.completeness, Completeness::CompleteForQuery);
        assert!(!report.unknown_frontiers.is_empty());
        assert!(report.human_summary.contains("Unknown"));
    }

    #[test]
    fn ifds_k013_history_metadata_only() {
        let report = run(OVERWRITE_BEFORE, OVERWRITE_AFTER, "x");
        assert!(
            report
                .after_graph
                .nodes
                .iter()
                .any(|node| node.span.path == "main.ts" && !node.fingerprint.is_empty())
        );
        assert!(!report.alignment.is_empty());
        assert!(!report.human_summary.contains("bug"));
    }

    #[test]
    fn ifds_k013_precise_change_record() {
        let report = run(OVERWRITE_BEFORE, OVERWRITE_AFTER, "x");
        assert!(
            report
                .deltas
                .iter()
                .any(|record| matches!(record.delta, FlowDelta::ValueSourceChanged { .. }))
        );
        assert!(report.deltas.iter().any(|record| !record.groups.is_empty()));
    }

    #[test]
    fn ifds_k013_human_summary_contract() {
        let report = run(OVERWRITE_BEFORE, OVERWRITE_AFTER, "x");
        assert!(
            report
                .human_summary
                .contains("Added selected-binding write")
        );
        assert!(report.human_summary.contains("Consumer input"));
        assert!(report.human_summary.contains("x = 2"));
        assert!(report.human_summary.contains("Earlier writer"));
        assert!(!report.human_summary.contains("no further impact"));
        let detached = run(
            "function example() {\n  let x = 1;\n  let y = x + 1;\n  let z = y + 2;\n  return z;\n}\n",
            "function example() {\n  let x = 1;\n  let y = x + 1;\n  let z = 1;\n  return z;\n}\n",
            "x",
        );
        assert!(detached.human_summary.contains("z = 1"));
        assert!(detached.human_summary.contains("no longer reaches return"));
        assert!(!detached.human_summary.contains("operation 8"));
        let z = run(
            "function example() {\n  let x = 1;\n  let y = x + 1;\n  let z = y + 2;\n  return z;\n}\n",
            "function example() {\n  let x = 1;\n  let y = x + 1;\n  let z = 1;\n  return z;\n}\n",
            "z",
        );
        assert!(z.human_summary.contains("z = 1"));
        assert!(!z.human_summary.contains("no longer reaches write"));
    }

    #[test]
    fn ifds_k013_new_input_propagates() {
        let report = run(
            "function example() {\n  let x = 1;\n  let y = x + 2;\n  let z = y + x + 4;\n  return z;\n}\n",
            "function example() {\n  let a = 5;\n  let x = a / 6;\n  let y = x + 2;\n  let z = y + x + 4;\n  return z;\n}\n",
            "x",
        );
        assert_eq!(report.completeness, Completeness::CompleteForQuery);
        assert!(report.unknown_frontiers.is_empty());
        assert!(report.deltas.iter().any(|record| {
            matches!(record.delta, FlowDelta::OperationChanged { .. })
                && record
                    .before_span
                    .as_ref()
                    .is_some_and(|span| span.start_line == 2)
                && record
                    .after_span
                    .as_ref()
                    .is_some_and(|span| span.start_line == 3)
        }));
        assert!(!report.deltas.iter().any(|record| matches!(
            record.delta,
            FlowDelta::WriteAdded { .. } | FlowDelta::WriteRemoved { .. }
        )));
        for line in [4, 5, 6] {
            assert!(
                report
                    .after_graph
                    .nodes
                    .iter()
                    .any(|node| node.span.start_line == line)
            );
        }
        assert!(report.after_graph.edges.iter().any(|edge| {
            report
                .after_graph
                .nodes
                .iter()
                .any(|node| node.id == edge.source && node.operation.contains("a = 5"))
                && report
                    .after_graph
                    .nodes
                    .iter()
                    .any(|node| node.id == edge.target && node.operation.contains("x = a / 6"))
        }));
        assert!(report.human_summary.contains("x = a / 6"));
        assert!(report.human_summary.contains("z = y + x + 4"));
        assert!(report.human_summary.contains("return z"));
    }

    #[test]
    fn ifds_k013_does_not_report_unrelated_declarations_as_changed() {
        let report = run(
            "function example() {\n  let x = 1;\n  let y = 2;\n  let z = x - y - 2;\n  return z;\n}\n",
            "function example() {\n  let a = 4;\n  let x = a - 1;\n  let z = x + a - 6;\n  return z;\n}\n",
            "x",
        );
        assert!(
            !report
                .human_summary
                .contains("Changed operation write `y = 2`")
        );
        assert!(report.human_summary.contains("write `a = 4`"));
        assert!(report.deltas.iter().all(|record| {
            !matches!(record.delta, FlowDelta::OperationChanged { .. })
                || record
                    .before_span
                    .as_ref()
                    .is_none_or(|span| span.start_line != 3)
        }));
    }

    #[test]
    fn ifds_k013_reports_sources_moved_into_expression_operand() {
        let report = run(
            "function example() {\n  let x = 1;\n  return x;\n}\n",
            "function example() {\n  let y = 1;\n  let x = 2;\n  return x + y;\n}\n",
            "x",
        );
        assert!(
            report
                .human_summary
                .contains("moved sources from (value) [write `x = 1`")
        );
        assert!(
            report
                .human_summary
                .contains("to (value.operands[0]) [write `x = 2`")
        );
        assert!(!report
            .human_summary
            .contains("return `return x + y;` at snippet.ts:4 (value) changed sources from [write `x = 1`] to []"));
    }

    #[test]
    fn ifds_k013_retargeted_consumer_chain() {
        let report = run(
            "function example() {\n  let x = 1;\n  let y = x + 1;\n  let z = y + 4;\n  return z;\n}\n",
            "function example() {\n  let x = 1;\n  let y = 2;\n  let z = x - y - 6;\n  return z;\n}\n",
            "x",
        );
        let reaches = |graph: &FlowGraph, source: &str, target: &str| {
            graph.edges.iter().any(|edge| {
                graph
                    .nodes
                    .iter()
                    .any(|node| node.id == edge.source && node.operation.contains(source))
                    && graph
                        .nodes
                        .iter()
                        .any(|node| node.id == edge.target && node.operation.contains(target))
            })
        };
        assert!(reaches(&report.before_graph, "x = 1", "y = x + 1"));
        assert!(reaches(&report.before_graph, "y = x + 1", "z = y + 4"));
        assert!(reaches(&report.after_graph, "x = 1", "z = x - y - 6"));
        assert!(!reaches(&report.after_graph, "y = 2", "z = x - y - 6"));
        assert_eq!(report.completeness, Completeness::CompleteForQuery);
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::AmbiguousMatch)
        );
        assert!(
            report
                .human_summary
                .contains("no longer carries the selected binding's value")
        );
        assert!(
            !report
                .human_summary
                .contains("Its selected-binding write was retained")
        );
    }

    #[test]
    fn ifds_k013_reordered_sources_keep_direct_flow() {
        let report = run(
            "function example() {\n  let x = 1;\n  let y = x + 2;\n  let z = x + y;\n  return z;\n}\n",
            "function example() {\n  let y = 1;\n  let x = 2;\n  let z = x + y;\n  return z;\n}\n",
            "x",
        );
        let reaches = |graph: &FlowGraph, source: &str, target: &str| {
            graph.edges.iter().any(|edge| {
                graph
                    .nodes
                    .iter()
                    .any(|node| node.id == edge.source && node.operation.contains(source))
                    && graph
                        .nodes
                        .iter()
                        .any(|node| node.id == edge.target && node.operation.contains(target))
            })
        };
        assert!(reaches(&report.before_graph, "x = 1", "y = x + 2"));
        assert!(reaches(&report.before_graph, "x = 1", "z = x + y"));
        assert!(reaches(&report.before_graph, "y = x + 2", "z = x + y"));
        assert!(reaches(&report.after_graph, "x = 2", "z = x + y"));
        assert!(!reaches(&report.after_graph, "x = 2", "y = 1"));
        assert!(!report.deltas.iter().any(|record| matches!(
            record.delta,
            FlowDelta::WriteAdded { .. } | FlowDelta::WriteRemoved { .. }
        )));
        assert!(report.deltas.iter().any(|record| matches!(
            record.delta,
            FlowDelta::FlowRemoved {
                relation: RelationKind::ValueDependency,
                ..
            }
        )));
        assert_eq!(report.completeness, Completeness::CompleteForQuery);
        assert!(report.diagnostics.is_empty());
        assert!(
            report
                .human_summary
                .contains("no longer carries the selected binding's value")
        );
    }
}
