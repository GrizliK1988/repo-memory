use super::*;
use crate::ifds::adapters::typescript::{index_bindings, lower_containing_procedure};
use crate::ifds::ir::EdgeKind;
use crate::ifds::model::{AnalysisLimits, DefinitionId, PathEndingKind, SnapshotId, SnapshotSide};
use crate::ifds::provenance::seed_entry_facts;
use crate::ifds::slicing::{SliceRequest, build_flow_slice};
use crate::ifds::solver::{IntraproceduralSolver, NoopSolverControl, Solver, SolverRequest};

fn lower(source: &str) -> (ProcedureIr, TypeScriptBindingIndex, BindingId, BranchPaths) {
    let snapshot = SnapshotId {
        side: SnapshotSide::Before,
        revision: "test".into(),
        content_id: "tree".into(),
    };
    let index = index_bindings(snapshot, "loop.ts", source).unwrap();
    let binding = index
        .bindings
        .iter()
        .find(|item| item.name == "x")
        .unwrap()
        .id
        .clone();
    let ir = lower_containing_procedure(source, &index, &binding)
        .unwrap()
        .procedure;
    let paths = BranchPaths::build(&ir, seed_entry_facts(&ir), 4096);
    assert!(!paths.truncated);
    (ir, index, binding, paths)
}

fn text<'a>(source: &'a str, ir: &ProcedureIr, node: &NodeId) -> &'a str {
    let span = ir.nodes[node].span.as_ref().unwrap();
    &source[span.byte_start as usize..span.byte_end as usize]
}

fn write(ir: &ProcedureIr, source: &str, code: &str) -> DefinitionId {
    ir.nodes
        .values()
        .find_map(|node| match &node.operation {
            Operation::Write { definition, .. } if text(source, ir, &node.id) == code => {
                Some(definition.clone())
            }
            _ => None,
        })
        .unwrap()
}

fn return_writes(
    ir: &ProcedureIr,
    paths: &BranchPaths,
    binding: &BindingId,
) -> BTreeSet<DefinitionId> {
    ir.nodes
        .values()
        .filter(|node| matches!(node.operation, Operation::Return { .. }))
        .flat_map(|node| paths.at.get(&node.id).into_iter().flatten())
        .flat_map(|state| state.facts.iter())
        .filter_map(|fact| match fact {
            Fact::LastWrite {
                place: Place::Binding(candidate),
                write,
            } if candidate == binding => Some(write.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn ifds_k016_while_vs_do() {
    let while_source =
        "function f(flag: boolean) { let x = 0; while (flag) { x = 1; flag = false; } return x; }";
    let (ir, index, binding, paths) = lower(while_source);
    assert_eq!(
        return_writes(&ir, &paths, &binding),
        BTreeSet::from([
            write(&ir, while_source, "x = 0"),
            write(&ir, while_source, "x = 1")
        ])
    );
    let summary = summarize_loops(&ir, &paths, &index, while_source);
    assert!(summary[0].may_skip && summary[0].may_enter);
    assert!(!summary[0].may_repeat && !summary[0].continuing_cycle);
    let do_source = "function f(flag: boolean) { let x = 0; do { x = 1; flag = false; } while (flag); return x; }";
    let (ir, index, binding, paths) = lower(do_source);
    assert_eq!(
        return_writes(&ir, &paths, &binding),
        BTreeSet::from([write(&ir, do_source, "x = 1")])
    );
    let summary = summarize_loops(&ir, &paths, &index, do_source);
    assert!(summary[0].may_enter);
    assert!(!summary[0].may_skip && !summary[0].may_repeat);
    let return_node = ir
        .nodes
        .values()
        .find(|node| matches!(node.operation, Operation::Return { .. }))
        .unwrap();
    assert_eq!(
        paths.conditions_for(&return_node.id, |_| true),
        BTreeSet::from([None])
    );
}

#[test]
fn ifds_k016_for_continue_target() {
    for source in [
        "function f(flag: boolean) { let x = 0; for (; flag; flag = false) { x = 1; continue; x = 2; } return x; }",
        "function f(flag: boolean) { let x = 0; while (flag) { x = 1; flag = false; continue; x = 2; } return x; }",
    ] {
        let (ir, _, binding, paths) = lower(source);
        let transfer = ir
            .nodes
            .values()
            .find(|node| {
                node.span
                    .as_ref()
                    .is_some_and(|_| text(source, &ir, &node.id) == "continue;")
            })
            .unwrap();
        assert!(
            ir.edges.iter().any(
                |edge| edge.source == transfer.id && edge.target == ir.loops[0].continue_target
            )
        );
        assert_eq!(
            return_writes(&ir, &paths, &binding),
            BTreeSet::from([write(&ir, source, "x = 0"), write(&ir, source, "x = 1")])
        );
        assert!(!paths.cycles.contains_key(&ir.loops[0].header));
    }
}

#[test]
fn ifds_k016_nested_break() {
    let source = "function f(flag: boolean) { let x = 0; while (flag) { while (true) { x = 1; break; x = 2; } x = 3; break; } return x; }";
    let (ir, _, binding, paths) = lower(source);
    assert_eq!(ir.loops.len(), 2);
    assert_eq!(
        return_writes(&ir, &paths, &binding),
        BTreeSet::from([write(&ir, source, "x = 0"), write(&ir, source, "x = 3")])
    );
    assert!(paths.cycle_backedges.is_empty());
    let inner = &ir.loops[0];
    let transfer = ir
        .nodes
        .values()
        .find(|node| {
            node.span.as_ref().is_some_and(|span| {
                span.byte_start > inner.span.byte_start
                    && span.byte_end < inner.span.byte_end
                    && text(source, &ir, &node.id) == "break;"
            })
        })
        .unwrap();
    assert!(
        ir.edges
            .iter()
            .any(|edge| edge.source == transfer.id && edge.target == inner.exit)
    );
}

#[test]
fn ifds_k016_loop_carried_kill() {
    let source = "function f(flag: boolean) { let x = 0; while (flag) { x = 1; x = x + 2; x = 3; } return x; }";
    let (ir, _, binding, paths) = lower(source);
    let killed = write(&ir, source, "x = x + 2");
    let body_read = ir
        .nodes
        .values()
        .find(|node| {
            matches!(node.operation, Operation::Read { .. }) && text(source, &ir, &node.id) == "x"
        })
        .unwrap();
    let states = &paths.at[&body_read.id];
    assert!(
        states
            .iter()
            .all(|state| state.facts.contains(&Fact::LastWrite {
                place: Place::Binding(binding.clone()),
                write: write(&ir, source, "x = 1")
            }))
    );
    assert!(
        states
            .iter()
            .all(|state| !state.facts.contains(&Fact::LastWrite {
                place: Place::Binding(binding.clone()),
                write: killed.clone()
            }))
    );
    assert!(!paths.cycle_backedges.is_empty());
    // An unchanged true Boolean input cannot become false on a later check.
    assert_eq!(
        return_writes(&ir, &paths, &binding),
        BTreeSet::from([write(&ir, source, "x = 0")])
    );
    let carried = "function f(flag: boolean) { let x = 0; while (flag) { x = x + 1; } return x; }";
    let (ir, index, _, paths) = lower(carried);
    assert!(
        summarize_loops(&ir, &paths, &index, carried)[0]
            .inputs
            .iter()
            .any(|input| input.name == "x" && input.carried)
    );
}

#[test]
fn ifds_k016_finite_evidence() {
    let source = "function f(flag: boolean) { let x = 0; while (flag) { x = x + 1; } return x; }";
    let (ir, _, binding, paths) = lower(source);
    assert!(paths.at.values().map(BTreeSet::len).sum::<usize>() < 300);
    assert_eq!(
        ir.nodes
            .values()
            .filter(|node| matches!(node.operation, Operation::Write { .. }))
            .count(),
        2
    );
    let limits = AnalysisLimits {
        time_ms: 30_000,
        memory_bytes: 256_000_000,
        processed_path_edges: 100_000,
        witnesses_per_relation: 10,
        output_nodes: 10_000,
    };
    let solved = IntraproceduralSolver::new()
        .solve(
            SolverRequest {
                procedure: &ir,
                entry_facts: seed_entry_facts(&ir),
                limits,
            },
            &NoopSolverControl,
        )
        .unwrap();
    let slice = build_flow_slice(SliceRequest {
        procedure: &ir,
        solved: &solved,
        selected_binding: &binding,
        declared_scope: "f",
    })
    .unwrap();
    assert!(
        slice
            .witnesses
            .iter()
            .any(|witness| !witness.backedges.is_empty())
    );
    assert!(
        slice
            .witnesses
            .iter()
            .all(|witness| witness.nodes.len() < 100)
    );
    for witness in &slice.witnesses {
        assert!(
            witness
                .backedges
                .iter()
                .all(
                    |(source, target)| ir.edges.iter().any(|edge| &edge.source == source
                        && &edge.target == target
                        && edge.kind == EdgeKind::LoopBack)
                )
        );
    }
}

#[test]
fn ifds_k016_cycle_not_exit() {
    let source = "function f() { let x = 1; for (;;) { x = x + 1; } }";
    let (ir, _, binding, paths) = lower(source);
    assert!(!paths.at.contains_key(ir.exits.first().unwrap()));
    let limits = AnalysisLimits {
        time_ms: 30_000,
        memory_bytes: 256_000_000,
        processed_path_edges: 100_000,
        witnesses_per_relation: 10,
        output_nodes: 10_000,
    };
    let solved = IntraproceduralSolver::new()
        .solve(
            SolverRequest {
                procedure: &ir,
                entry_facts: seed_entry_facts(&ir),
                limits,
            },
            &NoopSolverControl,
        )
        .unwrap();
    let slice = build_flow_slice(SliceRequest {
        procedure: &ir,
        solved: &solved,
        selected_binding: &binding,
        declared_scope: "f",
    })
    .unwrap();
    assert!(
        slice
            .path_endings
            .iter()
            .any(|ending| ending.kind == PathEndingKind::Cycle)
    );
    assert!(
        slice
            .path_endings
            .iter()
            .all(|ending| ending.kind != PathEndingKind::ExecutionExit)
    );
    assert!(!slice.extent.value_lifecycle_closed);
}

#[test]
fn ifds_k016_for_scope_and_unreachable_body() {
    let source =
        "function f() { let x = 0; for (let x = 1; false; x = x + 1) { x = 2; } return x; }";
    let (ir, index, binding, paths) = lower(source);
    assert_eq!(
        index
            .bindings
            .iter()
            .filter(|item| item.name == "x")
            .count(),
        2
    );
    assert_eq!(
        return_writes(&ir, &paths, &binding),
        BTreeSet::from([write(&ir, source, "x = 0")])
    );
    let summaries = summarize_loops(&ir, &paths, &index, source);
    assert!(!summaries[0].may_enter);
    assert!(
        summaries[0]
            .operations
            .iter()
            .all(|operation| operation.operation != "write")
    );
}

#[test]
fn ifds_k016_budget_keeps_loop_effects_uncertain() {
    let source = "function f(flag: boolean) { let x = 0; while (flag) { x = 1; } return x; }";
    let (ir, index, _, _) = lower(source);
    let paths = BranchPaths::build(&ir, seed_entry_facts(&ir), 1);
    assert!(paths.truncated);
    let summaries = summarize_loops(&ir, &paths, &index, source);
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].feasibility, Coverage::Partial);
    assert!(summaries[0].may_enter && summaries[0].may_repeat);
    assert!(
        summaries[0]
            .operations
            .iter()
            .any(|operation| operation.operation == "write")
    );
    assert!(
        !render_loop_summaries(&summaries, "after")
            .join("\n")
            .contains("proven unreachable")
    );
}
