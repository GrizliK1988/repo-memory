//! Public, snapshot-backed orchestration and evidence-based report rendering.

use super::adapters::typescript::{
    TypeScriptBindingIndex, alignable_bindings, index_bindings, lower_containing_procedure,
};
use super::compact::{VariableSourceReport, project_variable_sources};
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
    let candidates: Vec<_> = index
        .bindings
        .iter()
        .filter(|binding| {
            binding.name == selected.name && binding.enclosing_symbol == selected.enclosing_symbol
        })
        .collect();
    let same_role: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|binding| {
            (binding.kind == super::adapters::typescript::BindingKind::Parameter)
                == (selected.kind == super::adapters::typescript::BindingKind::Parameter)
        })
        .collect();
    let mut matches = if same_role.is_empty() {
        candidates.into_iter()
    } else {
        same_role.into_iter()
    };
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
    analyze_variable_flow_reports(query, provider, before_environment, after_environment)
        .map(|(full, _)| full)
}

/// Return the full evidence and its compact projection from the same analysis.
pub fn analyze_variable_flow_reports<P: SnapshotProvider>(
    query: VariableFlowQuery,
    provider: &P,
    before_environment: &AnalysisEnvironment,
    after_environment: &AnalysisEnvironment,
) -> Result<(VariableFlowReport, VariableSourceReport), AnalysisError> {
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
) -> Result<(VariableFlowReport, VariableSourceReport), AnalysisError> {
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
    let mut report = VariableFlowReport {
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
        human_summary: String::new(),
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
    let compact =
        project_variable_sources(&report, before_ir, after_ir, before_source, after_source);
    report.human_summary = compact.render_text();
    report.validate()?;
    compact.validate_with_full(&report)?;
    Ok((report, compact))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ifds::compact::{FindingKind, GuardClause, Observation, SourceDefinition};
    use crate::ifds::snapshots::{InMemorySnapshot, InMemorySnapshotProvider};

    fn selector(snapshot: SnapshotId, source: &str, name: &str) -> BindingSelector {
        let needle = format!("let {name}");
        let start = source
            .find(&needle)
            .map(|offset| offset + 4)
            .or_else(|| source.find(&format!("{name}:")))
            .unwrap();
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
        run_with_counterpart(before, after, name, true)
    }

    fn run_reports(
        before: &str,
        after: &str,
        name: &str,
    ) -> (VariableFlowReport, VariableSourceReport) {
        run_reports_with_counterpart(before, after, name, true)
    }

    fn run_with_counterpart(
        before: &str,
        after: &str,
        name: &str,
        explicit_counterpart: bool,
    ) -> VariableFlowReport {
        run_reports_with_counterpart(before, after, name, explicit_counterpart).0
    }

    fn run_reports_with_counterpart(
        before: &str,
        after: &str,
        name: &str,
        explicit_counterpart: bool,
    ) -> (VariableFlowReport, VariableSourceReport) {
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
            counterpart: explicit_counterpart.then(|| selector(after_id, after, name)),
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
        analyze_variable_flow_reports(query, &provider, &environment, &environment).unwrap()
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
        let (report, compact) = run_reports(OVERWRITE_BEFORE, OVERWRITE_AFTER, "x");
        assert_eq!(report.human_summary, compact.render_text());
        assert!(report.human_summary.contains("Before:"));
        assert!(report.human_summary.contains("After:"));
        assert!(report.human_summary.contains("x = 2"));
        assert!(report.human_summary.contains("return x"));
        assert!(
            report
                .human_summary
                .contains("remains in the code but no longer reaches this input")
        );
        assert!(compact.findings.iter().any(|finding| finding.kind == super::super::compact::FindingKind::SourceSetChanged));
        assert!(!report.human_summary.contains("no further impact"));
        let detached = run(
            "function example() {\n  let x = 1;\n  let y = x + 1;\n  let z = y + 2;\n  return z;\n}\n",
            "function example() {\n  let x = 1;\n  let y = x + 1;\n  let z = 1;\n  return z;\n}\n",
            "x",
        );
        assert!(
            detached.human_summary.contains("z = 1"),
            "{}",
            detached.human_summary
        );
        assert!(detached.human_summary.contains("return z"));
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
    fn ifds_k014_added_guard_report_preserves_path_conditions() {
        let (report, compact) = run_reports(
            "function f(flag: boolean) {\n  let x = 0;\n  return x;\n}\n",
            "function f(flag: boolean) {\n  let x = 0;\n  if (flag) x = 1;\n  return x;\n}\n",
            "x",
        );
        assert_eq!(report.human_summary, compact.render_text());
        assert!(report.human_summary.contains("x = 0"));
        assert!(
            report.human_summary.contains("x = 1` under flag"),
            "{}",
            report.human_summary
        );
        assert!(report.human_summary.contains("return x"));
        assert!(!compact.controls.is_empty());
        assert!(!report.human_summary.contains("Consumer input"));
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
    fn ifds_k013_local_parameter_transition_changes_value_origin() {
        let local = "function f() {\n  let x = 1;\n  let y = 2;\n  return x + y;\n}\n";
        let parameter = "function f(x: number) {\n  let y = 2;\n  return x + y;\n}\n";
        for explicit in [false, true] {
            let (report, compact) = run_reports_with_counterpart(local, parameter, "x", explicit);
            assert_eq!(report.completeness, Completeness::CompleteForQuery);
            assert!(
                report
                    .deltas
                    .iter()
                    .any(|record| matches!(record.delta, FlowDelta::WriteRemoved { .. }))
            );
            assert!(compact.findings.iter().any(|finding| finding.kind
                == super::super::compact::FindingKind::SourceSetChanged
                && finding.before_sources != finding.after_sources));
            assert!(report.human_summary.contains("function_input `x`"));
            assert!(report.diagnostics.is_empty());

            let (reverse, reverse_compact) =
                run_reports_with_counterpart(parameter, local, "x", explicit);
            assert_eq!(reverse.completeness, Completeness::CompleteForQuery);
            assert!(
                reverse
                    .deltas
                    .iter()
                    .any(|record| matches!(record.delta, FlowDelta::WriteAdded { .. }))
            );
            assert!(reverse_compact.findings.iter().any(|finding| finding.kind
                == super::super::compact::FindingKind::SourceSetChanged
                && finding.before_sources != finding.after_sources));
            assert!(reverse.human_summary.contains("function_input `x`"));
            assert!(reverse.diagnostics.is_empty());
        }
    }

    #[test]
    fn ifds_k013_reports_sources_moved_into_expression_operand() {
        let (report, compact) = run_reports(
            "function example() {\n  let x = 1;\n  return x;\n}\n",
            "function example() {\n  let y = 1;\n  let x = 2;\n  return x + y;\n}\n",
            "x",
        );
        assert!(
            compact
                .observations
                .iter()
                .any(|observation| observation.projection == "value.operands[0]")
        );
        assert!(compact.findings.iter().any(|finding| finding.kind == super::super::compact::FindingKind::SourceSetChanged));
        assert!(report.human_summary.contains("x = 2"));
        assert!(!report
            .human_summary
            .contains("return `return x + y;` at snippet.ts:4 (value) changed sources from [write `x = 1`] to []"));
    }

    #[test]
    fn ifds_k013_retargeted_consumer_chain() {
        let (report, compact) = run_reports(
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
        assert!(compact.findings.iter().any(|finding| finding.kind == super::super::compact::FindingKind::SourceSetChanged));
        assert!(
            !report
                .human_summary
                .contains("Its selected-binding write was retained")
        );
    }

    #[test]
    fn ifds_k013_reordered_sources_keep_direct_flow() {
        let (report, compact) = run_reports(
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
        assert!(compact.findings.iter().any(|finding| finding.kind == super::super::compact::FindingKind::SourceSetChanged));
    }

    #[test]
    fn ifds_k014_human_summary_names_added_early_return_flow() {
        let (report, compact) = run_reports(
            "function f(flag: boolean) { let x = 0; x = 1; x = 2; x = 3; x = 4; return x + 4; }",
            "function f(flag: boolean) { let x = 0; x = 1; x = 2; if (flag) return x; x = 3; x = 4; return x + 4; }",
            "x",
        );
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
        assert!(compact.findings.iter().any(|finding| finding.kind == super::super::compact::FindingKind::ObservationAdded));
        assert!(report.human_summary.contains("return x"));
        assert!(report.human_summary.contains("Use guard: flag"));
        assert!(report.human_summary.contains("!flag"));
    }

    fn compact_observation<'a>(compact: &'a VariableSourceReport, text: &str) -> &'a Observation {
        compact
            .observations
            .iter()
            .find(|observation| {
                observation
                    .after
                    .as_ref()
                    .or(observation.before.as_ref())
                    .is_some_and(|site| site.operation.contains(text))
            })
            .unwrap_or_else(|| panic!("observation {text:?} missing"))
    }

    fn compact_source<'a>(compact: &'a VariableSourceReport, text: &str) -> &'a SourceDefinition {
        compact
            .sources
            .iter()
            .find(|source| {
                source
                    .after
                    .as_ref()
                    .is_some_and(|site| site.operation.contains(text))
                    || source
                        .before
                        .as_ref()
                        .is_some_and(|site| site.operation.contains(text))
            })
            .unwrap_or_else(|| panic!("source {text:?} missing"))
    }

    fn clauses_apply(
        clauses: &BTreeSet<GuardClause>,
        outcomes: &BTreeMap<LogicalNodeId, bool>,
    ) -> bool {
        clauses.iter().any(|clause| {
            clause
                .terms
                .iter()
                .all(|term| outcomes.get(&term.control) == Some(&term.outcome))
        })
    }

    #[test]
    fn ifds_k041_sequential_guards() {
        let (full, compact) = run_reports(
            "function f(flag: boolean, flag2: boolean) { let x = 1; return x; }",
            "function f(flag: boolean, flag2: boolean) { let x = 1; if (flag) x = 2; if (flag2) x = 3; return x; }",
            "x",
        );
        let observation = compact_observation(&compact, "return x");
        let after = observation.after_state.as_ref().unwrap();
        assert_eq!(after.sources.len(), 3);
        assert_eq!(compact.controls.len(), 2);
        assert!(after.precedence.iter().any(|rule| {
            let earlier = compact
                .sources
                .iter()
                .find(|source| source.id == rule.earlier)
                .unwrap();
            let later = compact
                .sources
                .iter()
                .find(|source| source.id == rule.later)
                .unwrap();
            earlier.after.as_ref().unwrap().operation.contains("x = 2")
                && later.after.as_ref().unwrap().operation.contains("x = 3")
        }));
        let flag = compact
            .controls
            .iter()
            .find(|control| {
                control
                    .after
                    .as_ref()
                    .unwrap()
                    .operation
                    .contains("(flag) ")
            })
            .unwrap_or_else(|| {
                compact
                    .controls
                    .iter()
                    .find(|control| {
                        control
                            .after
                            .as_ref()
                            .unwrap()
                            .operation
                            .contains("(flag)\u{60}")
                    })
                    .unwrap()
            });
        let flag2 = compact
            .controls
            .iter()
            .find(|control| control.after.as_ref().unwrap().operation.contains("flag2"))
            .unwrap();
        for (first, second, expected) in [
            (false, false, "x = 1"),
            (true, false, "x = 2"),
            (false, true, "x = 3"),
            (true, true, "x = 3"),
        ] {
            let values = BTreeMap::from([(flag.id, first), (flag2.id, second)]);
            let active: Vec<_> = after
                .selections
                .iter()
                .filter(|selection| clauses_apply(selection.clauses.as_ref().unwrap(), &values))
                .collect();
            assert_eq!(active.len(), 1, "{first:?}/{second:?}: {active:?}");
            let writer = compact
                .sources
                .iter()
                .find(|source| source.id == active[0].source)
                .unwrap();
            assert!(writer.after.as_ref().unwrap().operation.contains(expected));
        }
        assert!(full.human_summary.len() < 1100);
        assert!(
            compact.to_canonical_json().unwrap().len() < full.to_canonical_json().unwrap().len()
        );
    }

    #[test]
    fn ifds_k041_guard_only_change() {
        let (full, compact) = run_reports(
            "function f(flag: boolean) { let x = 1; if (flag) x = 2; return x; }",
            "function f(flag: boolean) { let x = 1; if (!flag) x = 2; return x; }",
            "x",
        );
        let return_value = compact_observation(&compact, "return x");
        assert_eq!(
            return_value.before_state.as_ref().unwrap().sources,
            return_value.after_state.as_ref().unwrap().sources
        );
        assert!(
            compact
                .findings
                .iter()
                .any(|finding| finding.kind == FindingKind::SelectionChanged)
        );
        assert!(!full.deltas.iter().any(|record| matches!(
            record.delta,
            FlowDelta::WriteAdded { .. } | FlowDelta::WriteRemoved { .. }
        )));
        assert!(compact.controls.iter().any(|control| {
            control.before.as_ref().unwrap().operation.contains("flag")
                && control.after.as_ref().unwrap().operation.contains("!flag")
        }));
    }

    #[test]
    fn ifds_k041_reordered_priority() {
        let (_, compact) = run_reports(
            "function f(a: boolean, b: boolean) { let x = 1; if (a) x = 2; if (b) x = 3; return x; }",
            "function f(a: boolean, b: boolean) { let x = 1; if (b) x = 3; if (a) x = 2; return x; }",
            "x",
        );
        let observation = compact_observation(&compact, "return x");
        let before = observation.before_state.as_ref().unwrap();
        let after = observation.after_state.as_ref().unwrap();
        assert_eq!(before.sources, after.sources);
        assert_ne!(before.precedence, after.precedence);
        assert!(
            compact
                .findings
                .iter()
                .any(|finding| finding.kind == FindingKind::SelectionChanged)
        );
    }

    #[test]
    fn ifds_k041_nested_guards() {
        let (_, compact) = run_reports(
            "function f(a: boolean, b: boolean) { let x = 1; return x; }",
            "function f(a: boolean, b: boolean) { let x = 1; if (a) { if (b) x = 2; else x = 3; } return x; }",
            "x",
        );
        let two = compact_source(&compact, "x = 2");
        let three = compact_source(&compact, "x = 3");
        let two_guard = two.after_assignment_guard.as_ref().unwrap();
        let three_guard = three.after_assignment_guard.as_ref().unwrap();
        assert!(two_guard.iter().all(|clause| clause.terms.len() >= 2));
        assert!(three_guard.iter().all(|clause| clause.terms.len() >= 2));
        assert_ne!(two_guard, three_guard);
        assert!(
            three_guard
                .iter()
                .any(|clause| clause.terms.iter().any(|term| !term.outcome))
        );
    }

    #[test]
    fn ifds_k041_early_return() {
        let (full, compact) = run_reports(
            "function f(flag: boolean) { let x = 0; x = 1; x = 2; x = 3; return x + 4; }",
            "function f(flag: boolean) { let x = 0; x = 1; x = 2; if (flag) return x; x = 3; return x + 4; }",
            "x",
        );
        assert!(
            compact
                .findings
                .iter()
                .any(|finding| finding.kind == FindingKind::ObservationAdded)
        );
        assert!(
            compact
                .observations
                .iter()
                .filter(|observation| observation
                    .after
                    .as_ref()
                    .is_some_and(|site| site.operation.contains("return x")))
                .count()
                >= 2
        );
        assert!(full.human_summary.contains("Use guard: flag"));
        assert!(full.human_summary.contains("!flag"));
        let (repeated_full, repeated_compact) = run_reports(
            "function f(flag: boolean) { let x = 0; x = 1; x = 2; x = 3; return x; }",
            "function f(flag: boolean) { let x = 0; x = 1; x = 2; if (flag) return x; x = 3; return x; }",
            "x",
        );
        if repeated_full
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::AmbiguousMatch)
        {
            assert!(!repeated_compact.comparison_complete);
        }
    }

    #[test]
    fn ifds_k041_operand_and_origins() {
        let (_, compact) = run_reports(
            "function f(p: number) { let x = p; return x + 4; }",
            "function f(p: number) { let x = p + 1; return x + 4; }",
            "x",
        );
        let writer = compact_source(&compact, "x = p + 1");
        assert!(!writer.after_upstream.is_empty());
        assert!(
            writer
                .after_upstream
                .iter()
                .any(|id| compact.sources.iter().any(|source| source.id == *id
                    && source
                        .after
                        .as_ref()
                        .is_some_and(|site| site.operation.contains("p"))))
        );
        let operand = compact
            .observations
            .iter()
            .find(|observation| {
                observation.projection == "value.operands[0]"
                    && observation
                        .after
                        .as_ref()
                        .is_some_and(|site| site.operation.contains("return x + 4"))
            })
            .unwrap();
        assert!(
            operand
                .after_state
                .as_ref()
                .unwrap()
                .sources
                .contains(&writer.id)
        );
        assert!(!compact.observations.iter().any(|observation| {
            observation.projection == "value"
                && observation
                    .after
                    .as_ref()
                    .is_some_and(|site| site.operation.contains("return x + 4"))
                && observation
                    .after_state
                    .as_ref()
                    .is_some_and(|state| state.sources.contains(&writer.id))
        }));
    }

    #[test]
    fn ifds_k041_overwritten_and_copied() {
        let (_, copied) = run_reports(
            "function f(p: number, q: number) { let x = p; let y = x; x = 3; return y; }",
            "function f(p: number, q: number) { let x = q; let y = x; x = 3; return y; }",
            "x",
        );
        let old_writer = compact_source(&copied, "x = p");
        let copy = compact_source(&copied, "y = x");
        assert!(copy.before_upstream.contains(&old_writer.id));
        assert!(
            copied
                .findings
                .iter()
                .any(|finding| finding.kind == FindingKind::ExpressionChanged
                    || finding.kind == FindingKind::SourceSetChanged)
        );
        let (_, overwritten) = run_reports(
            "function f(p: number, q: number) { let x = p; x = 3; return x; }",
            "function f(p: number, q: number) { let x = q; x = 3; return x; }",
            "x",
        );
        let final_return = compact_observation(&overwritten, "return x");
        let final_sources = &final_return.after_state.as_ref().unwrap().sources;
        assert!(final_sources.contains(&compact_source(&overwritten, "x = 3").id));
        assert!(!final_sources.contains(&compact_source(&overwritten, "x = q").id));
    }

    #[test]
    fn ifds_k041_equal_rhs_distinct_writes() {
        let (_, compact) = run_reports(
            "function f() { let x = 1; return x; }",
            "function f() { let x = 1; x = 1; return x; }",
            "x",
        );
        let same_rhs: Vec<_> = compact
            .sources
            .iter()
            .filter(|source| {
                source
                    .after
                    .as_ref()
                    .is_some_and(|site| site.operation.contains("x = 1"))
            })
            .collect();
        assert_eq!(same_rhs.len(), 2);
        assert_ne!(same_rhs[0].id, same_rhs[1].id);
        let (_, self_assignment) = run_reports(
            "function f() { let x = 1; return x; }",
            "function f() { let x = 1; x = x; return x; }",
            "x",
        );
        let self_write = compact_source(&self_assignment, "x = x");
        assert!(!self_write.after_upstream.is_empty());
        assert!(
            self_assignment
                .findings
                .iter()
                .any(|finding| finding.after_sources.contains(&self_write.id)
                    || finding.kind == FindingKind::WriteAdded)
        );
    }

    #[test]
    fn ifds_k041_expression_only_change() {
        let (_, compact) = run_reports(
            "function f(p: number) { let x = p + 1; return x; }",
            "function f(p: number) { let x = p - 1; return x; }",
            "x",
        );
        assert!(
            compact
                .findings
                .iter()
                .any(|finding| finding.kind == FindingKind::ExpressionChanged)
        );
        assert!(compact.render_text().contains("p + 1"));
        assert!(compact.render_text().contains("p - 1"));
        assert!(!compact.findings.iter().any(|finding| matches!(
            finding.kind,
            FindingKind::WriteAdded | FindingKind::WriteRemoved
        )));
    }

    #[test]
    fn ifds_k041_observations_and_grouping() {
        let (_, compact) = run_reports(
            "function f() { let x = 1; let y = x; y = x; return y; }",
            "function f() { let x = 2; let y = x; y = x; return y; }",
            "x",
        );
        let copies: Vec<_> = compact
            .observations
            .iter()
            .filter(|observation| {
                observation
                    .after
                    .as_ref()
                    .is_some_and(|site| site.operation.contains("y = x"))
            })
            .collect();
        assert_eq!(copies.len(), 2);
        assert_ne!(copies[0].id, copies[1].id);
        assert!(
            compact
                .observation_groups
                .iter()
                .any(|group| group.members.len() >= 2)
        );
        assert!(
            compact
                .findings
                .iter()
                .any(|finding| finding.kind == FindingKind::ExpressionChanged
                    || finding.kind == FindingKind::SourceSetChanged)
        );
    }

    #[test]
    fn ifds_k041_uncertainty_and_scope() {
        let (partial_full, partial) = run_reports(
            "function f() { let x = 1; x = mystery(x); return x; }",
            "function f() { let x = 1; x = mystery(x); return x; }",
            "x",
        );
        assert_ne!(partial.analysis_coverage, Completeness::CompleteForQuery);
        assert!(!partial.comparison_complete);
        assert!(
            partial
                .observations
                .iter()
                .flat_map(|observation| [
                    observation.before_state.as_ref(),
                    observation.after_state.as_ref()
                ])
                .flatten()
                .any(|state| !state.exhaustive)
        );
        assert!(!partial_full.flow_extent.after.value_lifecycle_closed);

        let (complete_full, _) = run_reports(OVERWRITE_BEFORE, OVERWRITE_AFTER, "x");
        assert_eq!(complete_full.completeness, Completeness::CompleteForQuery);
        let mut ambiguous = complete_full.clone();
        ambiguous.diagnostics.insert(Diagnostic {
            code: DiagnosticCode::AmbiguousMatch,
            message: "two return sites can correspond".into(),
            snapshot: None,
            frontier: None,
            span: None,
            affected_flows: BTreeSet::new(),
        });
        assert!(!crate::ifds::compact::comparison_is_complete(&ambiguous));
        assert_eq!(ambiguous.before_graph, complete_full.before_graph);
        assert_eq!(ambiguous.after_graph, complete_full.after_graph);
    }

    #[test]
    fn ifds_k041_guard_versions() {
        let (_, compact) = run_reports(
            "function f(flag: boolean, other: boolean) { let x = 1; if (flag) x = 2; flag = other; if (flag) x = 3; return x; }",
            "function f(flag: boolean, other: boolean) { let x = 1; if (flag) x = 2; flag = other; if (flag) x = 4; return x; }",
            "x",
        );
        let guards: Vec<_> = compact
            .controls
            .iter()
            .filter(|control| {
                control
                    .after
                    .as_ref()
                    .is_some_and(|site| site.operation.contains("flag"))
            })
            .collect();
        assert!(guards.len() >= 2);
        assert_ne!(guards[0].id, guards[1].id);
        assert_ne!(
            guards[0].after_value.as_ref().unwrap().reaching_writes,
            guards[1].after_value.as_ref().unwrap().reaching_writes
        );
    }

    #[test]
    fn ifds_k041_shared_conditions() {
        let (_, compact) = run_reports(
            "function f(a: boolean, b: boolean) { let x = 1; return x; }",
            "function f(a: boolean, b: boolean) { let x = 1; if (a) x = 2; if (b) x = 3; return x; }",
            "x",
        );
        let third = compact_source(&compact, "x = 3");
        let observation = compact_observation(&compact, "return x");
        let selection = observation
            .after_state
            .as_ref()
            .unwrap()
            .selections
            .iter()
            .find(|selection| selection.source == third.id)
            .unwrap();
        let clauses = selection.clauses.as_ref().unwrap();
        assert_eq!(clauses.len(), 1);
        assert_eq!(clauses.iter().next().unwrap().terms.len(), 1);
        let (_, unsupported) = run_reports(
            "function f(a: boolean, b: boolean) { let x = 1; return x; }",
            "function f(a: boolean, b: boolean) { let x = 1; if (a && b) x = 2; return x; }",
            "x",
        );
        assert!(
            !unsupported.comparison_complete
                || unsupported.analysis_coverage != Completeness::CompleteForQuery
        );
    }

    #[test]
    fn ifds_k041_output_contract() {
        let (full, compact) = run_reports(OVERWRITE_BEFORE, OVERWRITE_AFTER, "x");
        let encoded = compact.to_canonical_json().unwrap();
        let decoded = VariableSourceReport::from_json(&encoded).unwrap();
        assert_eq!(decoded, compact);
        assert_eq!(encoded, decoded.to_canonical_json().unwrap());
        assert!(decoded.validate_with_full(&full).is_ok());
        assert_eq!(full.human_summary, compact.render_text());
        assert_eq!(
            VariableFlowReport::from_json(&full.to_canonical_json().unwrap()).unwrap(),
            full
        );
        let mut invalid = compact.clone();
        invalid.findings[0]
            .evidence
            .insert(crate::ifds::compact::EvidenceRef {
                report_id: compact.full_report_id.clone(),
                section: "deltas".into(),
                index: full.deltas.len(),
            });
        assert!(invalid.validate_with_full(&full).is_err());
    }

    #[test]
    fn ifds_k041_presentation_budget() {
        let (full, compact) = run_reports(
            "function f(a: boolean, b: boolean) { let x = 1; return x; }",
            "function f(a: boolean, b: boolean) { let x = 1; if (a) x = 2; if (b) x = 3; return x; }",
            "x",
        );
        assert!(!full.witnesses.is_empty());
        assert!(compact.presentation_complete);
        assert_eq!(compact.omitted_groups, 0);
        assert_eq!(
            compact
                .sources
                .iter()
                .map(|source| source.id)
                .collect::<BTreeSet<_>>()
                .len(),
            compact.sources.len()
        );
        assert_eq!(
            compact
                .controls
                .iter()
                .map(|control| control.id)
                .collect::<BTreeSet<_>>()
                .len(),
            compact.controls.len()
        );
        assert!(compact.render_text().len() < full.to_canonical_json().unwrap().len());
        assert_eq!(compact.analysis_coverage, full.completeness);
    }
}
