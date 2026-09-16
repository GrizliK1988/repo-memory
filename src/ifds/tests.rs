use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn set<T: Ord>(values: impl IntoIterator<Item = T>) -> BTreeSet<T> {
    values.into_iter().collect()
}

fn snapshot(side: SnapshotSide) -> SnapshotId {
    SnapshotId {
        side,
        revision: match side {
            SnapshotSide::Before => "rev-before",
            SnapshotSide::After => "rev-after",
        }
        .into(),
        content_id: match side {
            SnapshotSide::Before => "content-before",
            SnapshotSide::After => "content-after",
        }
        .into(),
    }
}

fn span(path: &str, byte_start: u64, byte_end: u64, line: u32) -> SourceSpan {
    SourceSpan {
        path: path.into(),
        byte_start,
        byte_end,
        start_line: line,
        end_line: line,
    }
}

fn report(reverse: bool) -> VariableFlowReport {
    let before = snapshot(SnapshotSide::Before);
    let after = snapshot(SnapshotSide::After);
    let bn = |local| NodeId::new(before.clone(), local);
    let an = |local| NodeId::new(after.clone(), local);
    let bb = BindingId::new(before.clone(), 1);
    let ab = BindingId::new(after.clone(), 1);
    let bd = DefinitionId::new(before.clone(), 1);
    let model = ModelVersion {
        name: "fetch-header".into(),
        version: "1".into(),
        digest: "sha256:model".into(),
    };
    let supported = EvidenceKind::Supported;
    let modeled = EvidenceKind::Modeled {
        model: model.clone(),
    };
    let unresolved = EvidenceKind::Unresolved {
        diagnostic: DiagnosticCode::UnresolvedCall,
    };

    let mut before_nodes = vec![
        FlowNode {
            id: bn(1),
            operation: "write".into(),
            span: span("src/é.ts", 0, 12, 1),
            enclosing_declaration: Some("f".into()),
            fingerprint: "write:x".into(),
        },
        FlowNode {
            id: bn(2),
            operation: "read".into(),
            span: span("src/é.ts", 13, 21, 2),
            enclosing_declaration: Some("f".into()),
            fingerprint: "read:x".into(),
        },
    ];
    let mut facts = vec![
        Fact::Zero,
        Fact::LastWrite {
            place: Place::Binding(bb.clone()),
            write: bd.clone(),
        },
        Fact::Origin {
            place: Place::Binding(bb.clone()),
            source: Source::FunctionInput(bb.clone()),
        },
    ];
    if reverse {
        before_nodes.reverse();
        facts.reverse();
    }

    let delta_values = vec![
        FlowDelta::NodeAdded { after: an(3) },
        FlowDelta::NodeRemoved { before: bn(3) },
        FlowDelta::SliceMembershipChanged {
            logical: LogicalNodeId(1),
            change: MembershipChange::Entered,
        },
        FlowDelta::OperationChanged {
            logical: LogicalNodeId(2),
            before: bn(1),
            after: an(1),
        },
        FlowDelta::WriteAdded { after: an(1) },
        FlowDelta::WriteRemoved { before: bn(1) },
        FlowDelta::FlowAdded {
            source: LogicalNodeId(1),
            target: LogicalNodeId(2),
            relation: RelationKind::Reaches,
            projection: None,
        },
        FlowDelta::FlowRemoved {
            source: LogicalNodeId(2),
            target: LogicalNodeId(3),
            relation: RelationKind::ValueDependency,
            projection: Some("arguments[0]".into()),
        },
        FlowDelta::FlowConditionChanged {
            source: LogicalNodeId(1),
            target: LogicalNodeId(2),
            before: "ready".into(),
            after: "ready && enabled".into(),
        },
        FlowDelta::ValueSourceChanged {
            consumer: LogicalNodeId(2),
            projection: "return".into(),
            before_sources: set([LogicalNodeId(1)]),
            after_sources: set([LogicalNodeId(3)]),
        },
        FlowDelta::AnalysisUnknown {
            subject: "call target".into(),
            side: Some(SnapshotSide::After),
            diagnostic: DiagnosticCode::UnresolvedCall,
        },
    ];
    let mut deltas: Vec<_> = delta_values
        .into_iter()
        .enumerate()
        .map(|(index, delta)| DeltaRecord {
            delta,
            groups: set([GroupId(index as u64)]),
            before_span: Some(span("src/é.ts", 0, 12, 1)),
            after_span: Some(span("src/é.ts", 0, 14, 1)),
            condition: Some("entry".into()),
        })
        .collect();
    if reverse {
        deltas.reverse();
    }

    let origins = Source::Write(bd.clone());
    let carrier = Place::Binding(bb.clone());
    let ending_kinds = [
        PathEndingKind::NoFurtherUse,
        PathEndingKind::ExecutionExit,
        PathEndingKind::ScopeBoundary,
        PathEndingKind::ModeledBoundary,
        PathEndingKind::UnknownBoundary,
        PathEndingKind::Cycle,
    ];
    let endings: Vec<_> = ending_kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| PathEnding {
            model: (kind == PathEndingKind::ModeledBoundary).then(|| model.clone()),
            diagnostic: (kind == PathEndingKind::UnknownBoundary)
                .then_some(DiagnosticCode::UnknownExternalEffect),
            kind,
            origin: origins.clone(),
            carrier: carrier.clone(),
            location: span("src/é.ts", 22, 30, 3),
            condition: Some("reachable".into()),
            context: "function f".into(),
            witness: WitnessId(index as u64),
        })
        .collect();

    let capabilities = CapabilitySet {
        stage: 0,
        capabilities: set(["contracts".into(), "ir".into()]),
        version: "stage-0".into(),
    };
    let limits = AnalysisLimits {
        time_ms: 120_000,
        memory_bytes: 4_294_967_296,
        processed_path_edges: 1_000_000,
        witnesses_per_relation: 3,
        output_nodes: 100_000,
    };
    let request = VariableFlowQuery {
        before: SnapshotHandle {
            id: before.clone(),
            repository_id: "repo".into(),
        },
        after: SnapshotHandle {
            id: after.clone(),
            repository_id: "repo".into(),
        },
        diff: RepositoryDiff {
            content_id: "diff-id".into(),
            changed_files: set(["src/é.ts".into()]),
            renames: BTreeMap::new(),
        },
        selected_binding: BindingSelector {
            snapshot: before.clone(),
            declaration: span("src/é.ts", 4, 6, 1),
            expected_name: Some("x".into()),
            expected_enclosing_symbol: Some("f".into()),
        },
        counterpart: Some(BindingSelector {
            snapshot: after.clone(),
            declaration: span("src/é.ts", 4, 6, 1),
            expected_name: Some("x".into()),
            expected_enclosing_symbol: Some("f".into()),
        }),
        entry: EntryPoint::ContainingFunction,
        capabilities: capabilities.clone(),
        summaries: set([model.clone()]),
        limits,
    };

    VariableFlowReport {
        schema_version: REPORT_SCHEMA_VERSION,
        analysis_version: "0.1.0".into(),
        snapshots: set([before.clone(), after.clone()]),
        query: ResolvedQuery {
            request,
            before_binding: Some(bb.clone()),
            after_binding: Some(ab),
        },
        entry_assumptions: set([EntryAssumption {
            name: "input".into(),
            value: None,
            source: Source::FunctionInput(bb.clone()),
        }]),
        before_graph: FlowGraph {
            nodes: set(before_nodes),
            edges: set([
                FlowEdge {
                    source: bn(1),
                    target: bn(2),
                    relation: RelationKind::Reaches,
                    projection: None,
                    condition: None,
                    evidence: supported.clone(),
                },
                FlowEdge {
                    source: bn(2),
                    target: bn(3),
                    relation: RelationKind::Argument,
                    projection: Some("arguments[1].header".into()),
                    condition: None,
                    evidence: modeled.clone(),
                },
                FlowEdge {
                    source: bn(3),
                    target: bn(4),
                    relation: RelationKind::Controls,
                    projection: None,
                    condition: Some("true".into()),
                    evidence: unresolved.clone(),
                },
            ]),
            facts: set(facts),
        },
        after_graph: FlowGraph {
            nodes: set([FlowNode {
                id: an(1),
                operation: "write".into(),
                span: span("src/é.ts", 0, 14, 1),
                enclosing_declaration: Some("f".into()),
                fingerprint: "write:x:new".into(),
            }]),
            edges: BTreeSet::new(),
            facts: set([Fact::Zero]),
        },
        alignment: set([Alignment {
            logical: LogicalNodeId(1),
            before: Some(bn(1)),
            after: Some(an(1)),
            evidence: "explicit declaration counterpart".into(),
        }]),
        deltas: set(deltas),
        witnesses: set([
            Witness {
                id: WitnessId(0),
                nodes: vec![bn(1), bn(2)],
                backedges: BTreeSet::new(),
                summary_expansions: BTreeSet::new(),
                evidence: supported,
            },
            Witness {
                id: WitnessId(1),
                nodes: vec![bn(2), bn(3)],
                backedges: BTreeSet::new(),
                summary_expansions: set(["fetch-header@1".into()]),
                evidence: modeled,
            },
            Witness {
                id: WitnessId(2),
                nodes: vec![bn(3), bn(4)],
                backedges: set([(bn(4), bn(3))]),
                summary_expansions: BTreeSet::new(),
                evidence: unresolved,
            },
        ]),
        diagnostics: set([Diagnostic {
            code: DiagnosticCode::UnresolvedCall,
            message: "call target unavailable".into(),
            snapshot: Some(before.clone()),
            frontier: Some(bn(3)),
            span: Some(span("src/é.ts", 22, 30, 3)),
            affected_flows: set(["downstream".into()]),
        }]),
        unknown_frontiers: set([UnknownFrontier {
            node: bn(3),
            next_operation: "dynamic call".into(),
            diagnostic: DiagnosticCode::UnresolvedCall,
            affected_direction: Direction::Downstream,
        }]),
        capabilities,
        summaries_used: set([model]),
        flow_extent: FlowExtent {
            before: SnapshotExtent {
                upstream: Coverage::Complete,
                downstream: Coverage::Partial,
                declared_scope: "function f".into(),
                source_boundaries: set(["function input".into()]),
                sink_boundaries: set(["request header".into()]),
                value_lifecycle_closed: false,
            },
            after: SnapshotExtent {
                upstream: Coverage::Unsupported,
                downstream: Coverage::Complete,
                declared_scope: "function f".into(),
                source_boundaries: BTreeSet::new(),
                sink_boundaries: BTreeSet::new(),
                value_lifecycle_closed: true,
            },
        },
        path_endings: set(endings),
        completeness: Completeness::Partial,
        region_completeness: BTreeMap::from([
            ("downstream".into(), Coverage::Partial),
            ("upstream".into(), Coverage::Complete),
        ]),
        stats: AnalysisStats {
            elapsed_ms: 7,
            peak_memory_bytes: 4096,
            processed_path_edges: 11,
            graph_nodes: 4,
            graph_edges: 3,
            witnesses_emitted: 3,
            witnesses_omitted: 1,
            limits_hit: set(["witness_display".into()]),
            graph_truncated: false,
            witnesses_truncated: true,
        },
    }
}

#[test]
fn ifds_k001_round_trip_report() {
    let original = report(false);
    let encoded = original.to_canonical_json().unwrap();
    let decoded = VariableFlowReport::from_json(&encoded).unwrap();
    assert_eq!(decoded, original);
    assert_eq!(decoded.path_endings.len(), 6);
    assert_eq!(decoded.deltas.len(), 11);
    assert!(
        encoded
            .windows("src/é.ts".len())
            .any(|window| window == "src/é.ts".as_bytes())
    );
}

#[test]
fn ifds_k001_deterministic_serialization() {
    let forward = report(false).to_canonical_json().unwrap();
    let reverse = report(true).to_canonical_json().unwrap();
    assert_eq!(forward, reverse);
}

#[test]
fn ifds_k001_snapshot_ids_do_not_alias() {
    let before = NodeId::new(snapshot(SnapshotSide::Before), 7);
    let after = NodeId::new(snapshot(SnapshotSide::After), 7);
    assert_ne!(before, after);
    let mapping = Alignment {
        logical: LogicalNodeId(99),
        before: Some(before.clone()),
        after: Some(after.clone()),
        evidence: "explicit".into(),
    };
    assert_eq!(mapping.before, Some(before));
    assert_eq!(mapping.after, Some(after));
}

#[test]
fn ifds_k001_ir_without_language_adapter() {
    let snap = snapshot(SnapshotSide::Before);
    let node = |local| NodeId::new(snap.clone(), local);
    let binding = Place::Binding(BindingId::new(snap.clone(), 1));
    let nodes = [
        IrNode {
            id: node(0),
            operation: Operation::Entry,
            span: None,
        },
        IrNode {
            id: node(1),
            operation: Operation::Write {
                target: binding.clone(),
                sources: BTreeSet::new(),
                definition: DefinitionId::new(snap.clone(), 1),
            },
            span: Some(span("src/a.ts", 0, 9, 1)),
        },
        IrNode {
            id: node(2),
            operation: Operation::Read {
                source: binding.clone(),
                result: Place::Temporary(node(20)),
            },
            span: Some(span("src/a.ts", 10, 18, 2)),
        },
        IrNode {
            id: node(3),
            operation: Operation::Exit,
            span: None,
        },
    ];
    let ir = ProcedureIr {
        id: ProcedureId::new(snap.clone(), 1),
        entry: node(0),
        exits: set([node(3)]),
        nodes: nodes
            .into_iter()
            .map(|item| (item.id.clone(), item))
            .collect(),
        edges: set([
            IrEdge {
                source: node(0),
                target: node(1),
                kind: EdgeKind::Normal,
                construct: None,
                assumption: None,
            },
            IrEdge {
                source: node(1),
                target: node(2),
                kind: EdgeKind::Normal,
                construct: None,
                assumption: None,
            },
            IrEdge {
                source: node(2),
                target: node(3),
                kind: EdgeKind::Normal,
                construct: None,
                assumption: None,
            },
        ]),
    };
    assert_eq!(ir.validate(), Ok(()));
}

#[test]
fn ifds_k001_coverage_is_not_lifecycle() {
    let value = report(false);
    assert_eq!(value.flow_extent.before.upstream, Coverage::Complete);
    assert!(!value.flow_extent.before.value_lifecycle_closed);
    assert!(
        value
            .path_endings
            .iter()
            .any(|ending| ending.kind == PathEndingKind::ScopeBoundary)
    );
}

#[test]
fn ifds_k001_reject_invalid_schema() {
    let mut unsupported = report(false);
    unsupported.schema_version = REPORT_SCHEMA_VERSION + 1;
    assert!(matches!(
        unsupported.to_canonical_json(),
        Err(SchemaError::UnsupportedVersion { .. })
    ));

    let valid = report(false).to_canonical_json().unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&valid).unwrap();
    json["schema_version"] = serde_json::json!(REPORT_SCHEMA_VERSION + 1);
    assert!(matches!(
        VariableFlowReport::from_json(&serde_json::to_vec(&json).unwrap()),
        Err(SchemaError::UnsupportedVersion { .. })
    ));

    let mut invalid_span = report(false);
    invalid_span
        .query
        .request
        .selected_binding
        .declaration
        .byte_start = 20;
    invalid_span
        .query
        .request
        .selected_binding
        .declaration
        .byte_end = 10;
    assert!(matches!(
        invalid_span.to_canonical_json(),
        Err(SchemaError::InvalidSpan { .. })
    ));

    invalid_span
        .query
        .request
        .selected_binding
        .declaration
        .byte_start = 0;
    invalid_span
        .query
        .request
        .selected_binding
        .declaration
        .start_line = 0;
    assert!(matches!(
        invalid_span.to_canonical_json(),
        Err(SchemaError::InvalidSpan { .. })
    ));
}
