use super::*;
use crate::ifds::adapters::typescript::{
    TypeScriptBindingIndex, TypeScriptLoweringResult, alignable_bindings, index_bindings,
    lower_containing_procedure,
};
use crate::ifds::model::{
    AnalysisLimits, FileChange, FileChangeKind, PathEndingKind, RepositoryDiff, SnapshotId,
    SnapshotSide,
};
use crate::ifds::provenance::seed_entry_facts;
use crate::ifds::slicing::{SliceRequest, build_flow_slice};
use crate::ifds::solver::{IntraproceduralSolver, NoopSolverControl, Solver, SolverRequest};

struct Pair {
    before_source: String,
    after_source: String,
    before: TypeScriptLoweringResult,
    after: TypeScriptLoweringResult,
    before_slice: SliceOutput,
    after_slice: SliceOutput,
    alignment: AlignmentResult,
}

fn snapshot(side: SnapshotSide) -> SnapshotId {
    SnapshotId {
        side,
        revision: format!("{side:?}"),
        content_id: format!("{side:?}-tree"),
    }
}

fn lower(side: SnapshotSide, source: &str) -> (TypeScriptBindingIndex, TypeScriptLoweringResult) {
    let index = index_bindings(snapshot(side), "main.ts", source).unwrap();
    let selected = index
        .bindings
        .iter()
        .find(|b| b.name == "x")
        .unwrap()
        .id
        .clone();
    let lowered = lower_containing_procedure(source, &index, &selected).unwrap();
    (index, lowered)
}

fn slice(lowered: &TypeScriptLoweringResult) -> SliceOutput {
    let solved = IntraproceduralSolver::new()
        .solve(
            SolverRequest {
                procedure: &lowered.procedure,
                entry_facts: seed_entry_facts(&lowered.procedure),
                limits: AnalysisLimits {
                    time_ms: 60_000,
                    memory_bytes: 100_000_000,
                    processed_path_edges: 100_000,
                    witnesses_per_relation: 100,
                    output_nodes: 10_000,
                },
            },
            &NoopSolverControl,
        )
        .unwrap();
    build_flow_slice(SliceRequest {
        procedure: &lowered.procedure,
        solved: &solved,
        selected_binding: &lowered.selected_binding,
        declared_scope: "function f",
    })
    .unwrap()
}

fn pair(before: &str, after: &str) -> Pair {
    let (before_index, old) = lower(SnapshotSide::Before, before);
    let (after_index, new) = lower(SnapshotSide::After, after);
    let alignment = crate::ifds::compare::align_procedures(
        &old.procedure,
        &new.procedure,
        &alignable_bindings(&before_index, &old.procedure),
        &alignable_bindings(&after_index, &new.procedure),
        &RepositoryDiff {
            before_content_id: "Before-tree".into(),
            after_content_id: "After-tree".into(),
            changes: BTreeSet::from([FileChange {
                kind: FileChangeKind::Modified,
                before_path: Some("main.ts".into()),
                after_path: Some("main.ts".into()),
                before_content_id: Some("old".into()),
                after_content_id: Some("new".into()),
            }]),
        },
        &old.selected_binding,
        Some(&new.selected_binding),
    )
    .unwrap();
    let before_slice = slice(&old);
    let after_slice = slice(&new);
    Pair {
        before_source: before.into(),
        after_source: after.into(),
        before: old,
        after: new,
        before_slice,
        after_slice,
        alignment,
    }
}

fn run(pair: &Pair) -> ComparisonOutput {
    compare_flow_slices(ComparisonInput {
        before: &pair.before_slice,
        after: &pair.after_slice,
        before_ir: &pair.before.procedure,
        after_ir: &pair.after.procedure,
        before_binding: Some(&pair.before.selected_binding),
        after_binding: Some(&pair.after.selected_binding),
        alignment: &pair.alignment,
        query_key: "main.ts:f:x",
        before_presence: &BTreeMap::new(),
        after_presence: &BTreeMap::new(),
        condition_proofs: &BTreeMap::new(),
    })
    .unwrap()
}

#[test]
fn ifds_k014_one_sided_write() {
    let (_, lowered) = lower(
        SnapshotSide::Before,
        "function f(flag: boolean) { let x = 0; if (flag) x = 1; return x; }",
    );
    let result = slice(&lowered);
    let return_id = lowered
        .procedure
        .nodes
        .values()
        .find(|node| matches!(node.operation, Operation::Return { .. }))
        .unwrap()
        .id
        .clone();
    let reaching: BTreeSet<_> = result
        .graph
        .edges
        .iter()
        .filter(|edge| edge.target == return_id && edge.relation == RelationKind::Reaches)
        .map(|edge| edge.condition.clone())
        .collect();
    assert_eq!(
        reaching,
        BTreeSet::from([Some("flag".into()), Some("!flag".into())])
    );
    assert_eq!(
        lowered
            .procedure
            .nodes
            .values()
            .filter(|node| matches!(node.operation, Operation::Write { .. }))
            .count(),
        2
    );
}

#[test]
fn ifds_k014_two_sided_nested() {
    let (_, lowered) = lower(
        SnapshotSide::Before,
        "function f(a: boolean, b: boolean) { let x = 0; if (a) { if (b) x = 1; else x = 2; } else x = 3; return x; }",
    );
    let result = slice(&lowered);
    let return_id = lowered
        .procedure
        .nodes
        .values()
        .find(|node| matches!(node.operation, Operation::Return { .. }))
        .unwrap()
        .id
        .clone();
    let conditions: BTreeSet<_> = result
        .graph
        .edges
        .iter()
        .filter(|edge| edge.target == return_id && edge.relation == RelationKind::Reaches)
        .filter_map(|edge| edge.condition.clone())
        .collect();
    assert_eq!(
        conditions,
        BTreeSet::from(["a && b".into(), "!b && a".into(), "!a".into()])
    );
}

#[test]
fn ifds_k014_early_return() {
    let (_, lowered) = lower(
        SnapshotSide::Before,
        "function f(flag: boolean) { let x = 0; if (flag) return x; x = 1; return x; }",
    );
    let result = slice(&lowered);
    let returns: Vec<_> = lowered
        .procedure
        .nodes
        .values()
        .filter(|node| matches!(node.operation, Operation::Return { .. }))
        .collect();
    assert_eq!(returns.len(), 2);
    let first: BTreeSet<_> = result
        .graph
        .edges
        .iter()
        .filter(|edge| edge.target == returns[0].id && edge.relation == RelationKind::Reaches)
        .filter_map(|edge| edge.condition.clone())
        .collect();
    let second: BTreeSet<_> = result
        .graph
        .edges
        .iter()
        .filter(|edge| edge.target == returns[1].id && edge.relation == RelationKind::Reaches)
        .filter_map(|edge| edge.condition.clone())
        .collect();
    assert_eq!(first, BTreeSet::from(["flag".into()]));
    assert_eq!(second, BTreeSet::from(["!flag".into()]));
}

#[test]
fn ifds_k014_guard_only_delta() {
    let pair = pair(
        "function f(flag: boolean) { let x = 0; if (flag) x = 1; return x; }",
        "function f(flag: boolean) { let x = 0; if (!flag) x = 1; return x; }",
    );
    let deltas = run(&pair);
    let changed_conditions: Vec<_> = deltas
        .deltas
        .iter()
        .filter_map(|delta| match &delta.delta {
            FlowDelta::FlowConditionChanged { relation, .. } => Some(relation.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        changed_conditions
            .iter()
            .filter(|relation| **relation == RelationKind::Reaches)
            .count(),
        2,
        "{deltas:?}"
    );
    assert!(
        changed_conditions.contains(&RelationKind::Controls),
        "{deltas:?}"
    );
    assert!(
        changed_conditions.contains(&RelationKind::MayWrite),
        "{deltas:?}"
    );
    assert!(!deltas.deltas.iter().any(|delta| matches!(
        delta.delta,
        FlowDelta::WriteAdded { .. }
            | FlowDelta::WriteRemoved { .. }
            | FlowDelta::ValueSourceChanged { .. }
    )));
}

#[test]
fn ifds_k014_control_not_copy() {
    let (_, lowered) = lower(
        SnapshotSide::Before,
        "function f(input: number) { let x = input; let y = 0; if (x > 0) y = 1; return y; }",
    );
    let result = slice(&lowered);
    assert!(
        result
            .graph
            .edges
            .iter()
            .any(|edge| edge.relation == RelationKind::Controls)
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::UnprovenPathFeasibility)
    );
    let y_write = lowered.procedure.nodes.values().filter(|node| matches!(&node.operation,
        Operation::Write { target: Place::Binding(binding), .. } if binding != &lowered.selected_binding))
        .next_back().unwrap();
    assert!(!result.graph.edges.iter().any(|edge| edge.target == y_write.id && edge.relation == RelationKind::ValueDependency
        && lowered.procedure.nodes.get(&edge.source).is_some_and(|node| matches!(&node.operation,
            Operation::Write { target: Place::Binding(binding), .. } if binding == &lowered.selected_binding))));
}

#[test]
fn ifds_k014_false_and_contradictory() {
    let (_, false_case) = lower(
        SnapshotSide::Before,
        "function f() { let x = 0; if (false) x = 1; return x; }",
    );
    let false_slice = slice(&false_case);
    let unreachable = false_case
        .procedure
        .nodes
        .values()
        .filter(|node| matches!(node.operation, Operation::Write { .. }))
        .next_back()
        .unwrap();
    assert!(
        !false_slice
            .graph
            .nodes
            .iter()
            .any(|node| node.id == unreachable.id)
    );
    let (_, contradiction) = lower(
        SnapshotSide::Before,
        "function f(flag: boolean) { let x = 0; if (flag) { if (!flag) x = 1; } return x; }",
    );
    let contradiction_slice = slice(&contradiction);
    let impossible = contradiction
        .procedure
        .nodes
        .values()
        .filter(|node| matches!(node.operation, Operation::Write { .. }))
        .next_back()
        .unwrap();
    assert!(
        !contradiction_slice
            .graph
            .nodes
            .iter()
            .any(|node| node.id == impossible.id)
    );
    let (_, unsupported) = lower(
        SnapshotSide::Before,
        "function f(input: number) { let x = 0; if (input > 0) x = 1; return x; }",
    );
    let unsupported_slice = slice(&unsupported);
    assert!(
        unsupported_slice
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == DiagnosticCode::UnprovenPathFeasibility })
    );
    assert!(unsupported_slice.graph.edges.iter().any(|edge| {
        matches!(
            edge.evidence,
            EvidenceKind::Unresolved {
                diagnostic: DiagnosticCode::UnprovenPathFeasibility
            }
        )
    }));
}

#[test]
fn ifds_k014_path_endings_independent() {
    let (_, lowered) = lower(
        SnapshotSide::Before,
        "function f(flag: boolean) { let x = 0; if (flag) return x; return 1; }",
    );
    let result = slice(&lowered);
    assert!(
        result
            .path_endings
            .iter()
            .any(|ending| ending.kind == PathEndingKind::ScopeBoundary
                && ending.condition.as_deref() == Some("flag"))
    );
    assert!(
        result
            .path_endings
            .iter()
            .any(|ending| ending.kind == PathEndingKind::NoFurtherUse
                && ending.condition.as_deref() == Some("!flag"))
    );
    assert!(!result.extent.value_lifecycle_closed);
}

fn deltas(output: &ComparisonOutput) -> Vec<&FlowDelta> {
    output.deltas.iter().map(|record| &record.delta).collect()
}

fn has(output: &ComparisonOutput, predicate: impl Fn(&FlowDelta) -> bool) -> bool {
    deltas(output).into_iter().any(predicate)
}

fn logical_at(pair: &Pair, side: SnapshotSide, text: &str) -> LogicalNodeId {
    let (source, graph) = match side {
        SnapshotSide::Before => (&pair.before_source, &pair.before_slice.graph),
        SnapshotSide::After => (&pair.after_source, &pair.after_slice.graph),
    };
    let node = graph
        .nodes
        .iter()
        .find(|n| {
            source
                .get(n.span.byte_start as usize..n.span.byte_end as usize)
                .is_some_and(|snippet| snippet == text)
        })
        .unwrap_or_else(|| {
            panic!(
                "missing node {text:?}; nodes: {:?}",
                graph
                    .nodes
                    .iter()
                    .map(|n| source.get(n.span.byte_start as usize..n.span.byte_end as usize))
                    .collect::<Vec<_>>()
            )
        });
    pair.alignment
        .nodes
        .iter()
        .find(|a| match side {
            SnapshotSide::Before => a.before.as_ref() == Some(&node.id),
            SnapshotSide::After => a.after.as_ref() == Some(&node.id),
        })
        .unwrap()
        .logical
}

#[test]
fn ifds_k012_initializer_only() {
    let p = pair(
        "function f() { let x = 1; return x; }",
        "function f() { let x = 2; return x; }",
    );
    let result = run(&p);
    let a = logical_at(&p, SnapshotSide::Before, "x = 1");
    assert!(has(
        &result,
        |d| matches!(d, FlowDelta::OperationChanged { logical, .. } if *logical == a)
    ));
    assert!(has(
        &result,
        |d| matches!(d, FlowDelta::ValueSourceChanged { changed_operations, .. } if changed_operations.contains(&a))
    ));
    assert!(!has(&result, |d| matches!(
        d,
        FlowDelta::NodeAdded { .. }
            | FlowDelta::NodeRemoved { .. }
            | FlowDelta::WriteAdded { .. }
            | FlowDelta::WriteRemoved { .. }
            | FlowDelta::FlowAdded { .. }
            | FlowDelta::FlowRemoved { .. }
    )));
}

#[test]
fn ifds_k012_insert_remove_overwrite() {
    let old = "function f() { let x = 1; return x; }";
    let new = "function f() { let x = 1; x = 2; return x; }";
    for (before, after, added) in [(old, new, true), (new, old, false)] {
        let p = pair(before, after);
        let result = run(&p);
        assert!(has(&result, |d| if added {
            matches!(d, FlowDelta::NodeAdded { .. })
        } else {
            matches!(d, FlowDelta::NodeRemoved { .. })
        }));
        assert!(has(&result, |d| if added {
            matches!(d, FlowDelta::WriteAdded { .. })
        } else {
            matches!(d, FlowDelta::WriteRemoved { .. })
        }));
        assert!(has(&result, |d| matches!(d, FlowDelta::FlowAdded { .. })));
        assert!(has(&result, |d| matches!(d, FlowDelta::FlowRemoved { .. })));
        let a = logical_at(&p, SnapshotSide::Before, "x = 1");
        assert!(!has(
            &result,
            |d| matches!(d, FlowDelta::WriteRemoved { before } if p.alignment.nodes.iter().any(|alignment| alignment.logical == a && alignment.before.as_ref() == Some(before)))
        ));
    }
    let self_copy = pair(old, "function f() { let x = 1; x = x; return x; }");
    let output = run(&self_copy);
    assert!(has(&output, |d| matches!(d, FlowDelta::WriteAdded { .. })));
    assert!(has(&output, |d| matches!(
        d,
        FlowDelta::ValueSourceChanged { .. }
    )));
}

#[test]
fn ifds_k012_retarget_and_slice() {
    let old = "function f() { let x = 1; let y = 0; x = 2; return x; }";
    let new = "function f() { let x = 1; let y = 0; y = 2; return x; }";
    for (before, after, removed) in [(old, new, true), (new, old, false)] {
        let result = run(&pair(before, after));
        assert!(has(&result, |d| matches!(
            d,
            FlowDelta::OperationChanged { .. }
        )));
        assert!(has(
            &result,
            |d| matches!(d, FlowDelta::SliceMembershipChanged { change, .. } if *change == if removed { MembershipChange::Left } else { MembershipChange::Entered })
        ));
        assert!(has(&result, |d| if removed {
            matches!(d, FlowDelta::WriteRemoved { .. })
        } else {
            matches!(d, FlowDelta::WriteAdded { .. })
        }));
        assert!(!has(&result, |d| matches!(
            d,
            FlowDelta::NodeAdded { .. } | FlowDelta::NodeRemoved { .. }
        )));
    }
}

#[test]
fn ifds_k012_existing_input_switch() {
    let p = pair(
        "function f(p: number, q: number) { let x = p; return x; }",
        "function f(p: number, q: number) { let x = q; return x; }",
    );
    let result = run(&p);
    assert!(has(
        &result,
        |d| matches!(d, FlowDelta::ValueSourceChanged { before_bindings, after_bindings, .. } if before_bindings != after_bindings)
    ));
    assert!(!has(&result, |d| matches!(
        d,
        FlowDelta::NodeAdded { .. } | FlowDelta::NodeRemoved { .. }
    )));
}

#[test]
fn ifds_k012_no_downstream_change() {
    let p = pair(
        "function f() { let x = 1; x = 2; return x; }",
        "function f() { let x = 3; x = 2; return x; }",
    );
    let result = run(&p);
    assert!(has(&result, |d| matches!(
        d,
        FlowDelta::OperationChanged { .. }
    )));
    let return_node = logical_at(&p, SnapshotSide::Before, "return x;");
    assert!(!has(
        &result,
        |d| matches!(d, FlowDelta::ValueSourceChanged { consumer, .. } if *consumer == return_node)
    ));
    let trivia = pair(
        "function f() { let x = 1; return x; }",
        "function f() { let x = 1; /* note */ return x; }",
    );
    assert!(run(&trivia).deltas.is_empty(), "{:?}", run(&trivia).deltas);
    let unrelated = pair(
        "function f() { let y = 2; let x = 1; return x; }",
        "function f() { let y = 3; let x = 1; return x; }",
    );
    assert!(run(&unrelated).deltas.is_empty());
}

#[test]
fn ifds_k012_presence_truth_table() {
    let mut p = pair(
        "function f() { let x = 1; return x; }",
        "function f() { let x = 1; return x; }",
    );
    let edge = p.before_slice.graph.edges.iter().next().unwrap().clone();
    let ids = node_ids(&p.alignment, SnapshotSide::Before);
    let key = relation_key(&edge, &ids).unwrap();
    for old in [
        RelationPresence::Present,
        RelationPresence::Absent,
        RelationPresence::Unknown,
    ] {
        for new in [
            RelationPresence::Present,
            RelationPresence::Absent,
            RelationPresence::Unknown,
        ] {
            p.before_slice.graph.edges.clear();
            p.after_slice.graph.edges.clear();
            if old == RelationPresence::Present {
                p.before_slice.graph.edges.insert(edge.clone());
            }
            if new == RelationPresence::Present {
                let after_ids = node_ids(&p.alignment, SnapshotSide::After);
                let source = after_ids
                    .iter()
                    .find(|(_, id)| **id == key.source)
                    .unwrap()
                    .0
                    .clone();
                let target = after_ids
                    .iter()
                    .find(|(_, id)| **id == key.target)
                    .unwrap()
                    .0
                    .clone();
                p.after_slice.graph.edges.insert(FlowEdge {
                    source,
                    target,
                    ..edge.clone()
                });
            }
            let before_proof = BTreeMap::from([(key.clone(), old)]);
            let after_proof = BTreeMap::from([(key.clone(), new)]);
            let output = compare_flow_slices(ComparisonInput {
                before: &p.before_slice,
                after: &p.after_slice,
                before_ir: &p.before.procedure,
                after_ir: &p.after.procedure,
                before_binding: Some(&p.before.selected_binding),
                after_binding: Some(&p.after.selected_binding),
                alignment: &p.alignment,
                query_key: "truth",
                before_presence: &before_proof,
                after_presence: &after_proof,
                condition_proofs: &BTreeMap::new(),
            })
            .unwrap();
            assert_eq!(
                has(&output, |d| matches!(d, FlowDelta::FlowAdded { .. })),
                old == RelationPresence::Absent && new == RelationPresence::Present
            );
            assert_eq!(
                has(&output, |d| matches!(d, FlowDelta::FlowRemoved { .. })),
                old == RelationPresence::Present && new == RelationPresence::Absent
            );
            assert_eq!(
                has(
                    &output,
                    |d| matches!(d, FlowDelta::AnalysisUnknown { subject, .. } if subject.starts_with("presence:"))
                ),
                old == RelationPresence::Unknown || new == RelationPresence::Unknown
            );
        }
    }
}

#[test]
fn ifds_k012_changed_origin_chain() {
    let old = "function f() { let x = 1; const copy = x; x = 0; const result = copy + 1; return result; }";
    let new = "function f() { let x = 2; const copy = x; x = 0; const result = copy + 1; return result; }";
    let p = pair(old, new);
    let result = run(&p);
    let a = logical_at(&p, SnapshotSide::Before, "x = 1");
    assert_eq!(deltas(&result).iter().filter(|d| matches!(d, FlowDelta::ValueSourceChanged { changed_operations, .. } if changed_operations.contains(&a))).count(), 3);
    assert!(!has(&result, |d| matches!(
        d,
        FlowDelta::FlowAdded { .. } | FlowDelta::FlowRemoved { .. }
    )));
}

#[test]
fn ifds_k012_reordered_operation_flow() {
    let p = pair(
        "function f() { let x = 0; x = 1; x = 2; return x; }",
        "function f() { let x = 0; x = 2; x = 1; return x; }",
    );
    let result = run(&p);
    assert!(has(&result, |d| matches!(
        d,
        FlowDelta::FlowAdded { .. }
            | FlowDelta::FlowRemoved { .. }
            | FlowDelta::ValueSourceChanged { .. }
    )));
    assert!(!has(&result, |d| matches!(
        d,
        FlowDelta::NodeAdded { .. }
            | FlowDelta::NodeRemoved { .. }
            | FlowDelta::OperationChanged { .. }
    )));
}

#[test]
fn ifds_k012_condition_and_group_identity() {
    let mut p = pair(
        "function f() { let x = 1; return x; }",
        "function f() { let x = 2; return x; }",
    );
    let old_edge = p.before_slice.graph.edges.iter().next().unwrap().clone();
    let new_edge = p.after_slice.graph.edges.iter().next().unwrap().clone();
    p.before_slice.graph.edges.clear();
    p.after_slice.graph.edges.clear();
    p.before_slice.graph.edges.insert(FlowEdge {
        condition: Some("ready".into()),
        ..old_edge.clone()
    });
    p.before_slice.graph.edges.insert(FlowEdge {
        condition: Some("fallback".into()),
        ..old_edge.clone()
    });
    p.after_slice.graph.edges.insert(FlowEdge {
        condition: Some("ready && enabled".into()),
        ..new_edge.clone()
    });
    p.after_slice.graph.edges.insert(FlowEdge {
        condition: Some("fallback".into()),
        ..new_edge.clone()
    });
    let key = relation_key(&old_edge, &node_ids(&p.alignment, SnapshotSide::Before)).unwrap();
    let proofs = BTreeMap::from([(key, ConditionComparison::Different)]);
    let result = compare_flow_slices(ComparisonInput {
        before: &p.before_slice,
        after: &p.after_slice,
        before_ir: &p.before.procedure,
        after_ir: &p.after.procedure,
        before_binding: Some(&p.before.selected_binding),
        after_binding: Some(&p.after.selected_binding),
        alignment: &p.alignment,
        query_key: "condition",
        before_presence: &BTreeMap::new(),
        after_presence: &BTreeMap::new(),
        condition_proofs: &proofs,
    })
    .unwrap();
    assert_eq!(
        deltas(&result)
            .iter()
            .filter(|d| matches!(d, FlowDelta::FlowConditionChanged { .. }))
            .count(),
        1
    );
    assert!(!has(&result, |d| matches!(
        d,
        FlowDelta::FlowAdded { .. } | FlowDelta::FlowRemoved { .. }
    )));
    assert!(
        result
            .deltas
            .iter()
            .filter(|record| matches!(record.delta, FlowDelta::ValueSourceChanged { .. }))
            .all(|record| record.groups.len() == 1)
    );
}

#[test]
fn ifds_k012_ambiguous_alignment() {
    let p = pair(
        "function f() { let x = 1; foo(x); foo(x); return x; }",
        "function f() { let x = 1; foo(x); foo(x); foo(x); return x; }",
    );
    assert!(!p.alignment.ambiguities.is_empty());
    let result = run(&p);
    assert!(has(&result, |delta| matches!(
        delta,
        FlowDelta::AnalysisUnknown {
            diagnostic: DiagnosticCode::AmbiguousMatch,
            ..
        }
    )));
    assert!(!has(&result, |delta| matches!(
        delta,
        FlowDelta::NodeAdded { .. } | FlowDelta::NodeRemoved { .. }
    )));
}
