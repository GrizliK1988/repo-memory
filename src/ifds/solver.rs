//! Deterministic single-procedure fixed-point solving.

use crate::ifds::ir::{EdgeKind, IrEdge, ProcedureIr};
use crate::ifds::model::{AnalysisLimits, AnalysisStats, Diagnostic, Fact, NodeId};
use crate::ifds::provenance::{
    ProcedureFlowFunctions, TransferEvidence, TransferOutcome, UnsupportedTransferKind,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolverRequest<'a> {
    pub procedure: &'a ProcedureIr,
    /// The solver uses exactly these facts. Callers opt into parameter and Zero
    /// seeding with `seed_entry_facts` when that is the desired analysis.
    pub entry_facts: BTreeSet<Fact>,
    /// K009 interprets these limits. K008 exposes deterministic counters and
    /// cancellation checkpoints without imposing a limit policy itself.
    pub limits: AnalysisLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FactState {
    pub node: NodeId,
    pub fact: Fact,
}

/// A supported one-procedure transition that can later be assembled into a
/// witness. Alternative predecessors are retained rather than overwritten.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FactPredecessor {
    pub source: FactState,
    pub edge: IrEdge,
    pub destination: FactState,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnsupportedFlow {
    Operation {
        node: NodeId,
        operation: UnsupportedTransferKind,
    },
    Edge(Box<IrEdge>),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SolverCounters {
    /// Number of supported `(CFG edge, incoming fact)` transfers attempted.
    pub processed_path_edges: u64,
    /// Number of distinct `(node, fact)` states discovered so far.
    pub discovered_states: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolverCancellation {
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolverTermination {
    FixedPoint,
    Cancelled(SolverCancellation),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolverOutput {
    /// Incoming facts at every reachable operation.
    pub facts_at: BTreeMap<NodeId, BTreeSet<Fact>>,
    pub predecessors: BTreeSet<FactPredecessor>,
    pub evidence: BTreeSet<TransferEvidence>,
    pub unsupported: BTreeSet<UnsupportedFlow>,
    pub termination: SolverTermination,
    pub diagnostics: BTreeSet<Diagnostic>,
    pub stats: AnalysisStats,
}

/// Mutable execution policy is deliberately separate from `SolverRequest` so
/// requests remain ordinary declarative values. Implementations may inspect a
/// fake clock, an atomic cancellation flag, or deterministic counters.
pub trait SolverControl {
    fn checkpoint(&self, counters: &SolverCounters) -> Option<SolverCancellation>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoopSolverControl;

impl SolverControl for NoopSolverControl {
    fn checkpoint(&self, _counters: &SolverCounters) -> Option<SolverCancellation> {
        None
    }
}

pub trait Solver {
    fn solve(
        &self,
        request: SolverRequest<'_>,
        control: &dyn SolverControl,
    ) -> Result<SolverOutput, SolverError>;
}

pub trait FlowFunction {
    fn transfer(&self, node: &NodeId, fact: &Fact) -> TransferOutcome;

    fn evidence(&self, _node: &NodeId, _fact: &Fact) -> BTreeSet<TransferEvidence> {
        BTreeSet::new()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IntraproceduralSolver;

impl IntraproceduralSolver {
    pub fn new() -> Self {
        Self
    }

    fn solve_with_order(
        &self,
        request: SolverRequest<'_>,
        control: &dyn SolverControl,
        order: WorklistOrder,
    ) -> Result<SolverOutput, SolverError> {
        request
            .procedure
            .validate()
            .map_err(|error| SolverError::InvalidIr(error.to_string()))?;

        let flows = ProcedureFlowFunctions::new(request.procedure);
        let mut outgoing: BTreeMap<NodeId, Vec<IrEdge>> = request
            .procedure
            .nodes
            .keys()
            .cloned()
            .map(|node| (node, Vec::new()))
            .collect();
        for edge in &request.procedure.edges {
            outgoing
                .get_mut(&edge.source)
                .expect("validated edge source must exist")
                .push(edge.clone());
        }
        if order == WorklistOrder::Reverse {
            for edges in outgoing.values_mut() {
                edges.reverse();
            }
        }

        let mut facts_at = BTreeMap::<NodeId, BTreeSet<Fact>>::new();
        let entry_set = facts_at.entry(request.procedure.entry.clone()).or_default();
        let mut worklist = VecDeque::new();
        for fact in request.entry_facts {
            entry_set.insert(fact.clone());
            worklist.push_back(FactState {
                node: request.procedure.entry.clone(),
                fact,
            });
        }

        let mut counters = SolverCounters {
            processed_path_edges: 0,
            discovered_states: worklist.len() as u64,
        };
        let mut predecessors = BTreeSet::new();
        let mut evidence = BTreeSet::new();
        let mut unsupported = BTreeSet::new();
        let mut cancellation = None;

        'worklist: while !worklist.is_empty() {
            if let Some(reason) = control.checkpoint(&counters) {
                cancellation = Some(reason);
                break;
            }
            let state = match order {
                WorklistOrder::Fifo => worklist.pop_front(),
                WorklistOrder::Reverse => worklist.pop_back(),
            }
            .expect("non-empty worklist must yield a state");

            evidence.extend(flows.evidence(&state.node, &state.fact));
            let transferred = match flows.transfer(&state.node, &state.fact) {
                TransferOutcome::Supported(facts) => facts,
                TransferOutcome::Unsupported {
                    preserved,
                    operation,
                } => {
                    unsupported.insert(UnsupportedFlow::Operation {
                        node: state.node.clone(),
                        operation,
                    });
                    preserved
                }
            };

            for edge in &outgoing[&state.node] {
                if !is_intraprocedural_edge(&edge.kind) {
                    unsupported.insert(UnsupportedFlow::Edge(Box::new(edge.clone())));
                    continue;
                }
                if let Some(reason) = control.checkpoint(&counters) {
                    cancellation = Some(reason);
                    break 'worklist;
                }
                counters.processed_path_edges = counters.processed_path_edges.saturating_add(1);
                for fact in &transferred {
                    let destination = FactState {
                        node: edge.target.clone(),
                        fact: fact.clone(),
                    };
                    predecessors.insert(FactPredecessor {
                        source: state.clone(),
                        edge: edge.clone(),
                        destination: destination.clone(),
                    });
                    if facts_at
                        .entry(edge.target.clone())
                        .or_default()
                        .insert(fact.clone())
                    {
                        counters.discovered_states = counters.discovered_states.saturating_add(1);
                        worklist.push_back(destination);
                    }
                }
            }
        }

        let termination =
            cancellation.map_or(SolverTermination::FixedPoint, SolverTermination::Cancelled);
        let graph_nodes = facts_at.values().filter(|facts| !facts.is_empty()).count() as u64;
        let graph_edges = predecessors.len() as u64;
        Ok(SolverOutput {
            facts_at,
            predecessors,
            evidence,
            unsupported,
            termination,
            diagnostics: BTreeSet::new(),
            stats: AnalysisStats {
                elapsed_ms: 0,
                peak_memory_bytes: 0,
                processed_path_edges: counters.processed_path_edges,
                graph_nodes,
                graph_edges,
                witnesses_emitted: 0,
                witnesses_omitted: 0,
                limits_hit: BTreeSet::new(),
                graph_truncated: false,
                witnesses_truncated: false,
            },
        })
    }
}

impl Solver for IntraproceduralSolver {
    fn solve(
        &self,
        request: SolverRequest<'_>,
        control: &dyn SolverControl,
    ) -> Result<SolverOutput, SolverError> {
        self.solve_with_order(request, control, WorklistOrder::Fifo)
    }
}

fn is_intraprocedural_edge(kind: &EdgeKind) -> bool {
    matches!(
        kind,
        EdgeKind::Normal | EdgeKind::Branch { .. } | EdgeKind::LoopBack
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorklistOrder {
    Fifo,
    Reverse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolverError {
    InvalidIr(String),
    Internal(String),
}

impl fmt::Display for SolverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIr(reason) => write!(f, "invalid solver IR: {reason}"),
            Self::Internal(reason) => write!(f, "solver failed: {reason}"),
        }
    }
}

impl std::error::Error for SolverError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ifds::adapters::typescript::{index_bindings, lower_containing_procedure};
    use crate::ifds::fixtures::evaluate_reference;
    use crate::ifds::ir::{IrNode, Operation, UnknownEffectKind};
    use crate::ifds::model::{
        BindingId, DefinitionId, Place, ProcedureId, SnapshotId, SnapshotSide, Source,
    };
    use crate::ifds::provenance::seed_entry_facts;

    fn snapshot() -> SnapshotId {
        SnapshotId {
            side: SnapshotSide::Before,
            revision: "k008".into(),
            content_id: "k008-content".into(),
        }
    }

    fn node(local: u64) -> NodeId {
        NodeId::new(snapshot(), local)
    }

    fn definition(local: u64) -> DefinitionId {
        DefinitionId::new(snapshot(), local)
    }

    fn binding(local: u64) -> Place {
        Place::Binding(BindingId::new(snapshot(), local))
    }

    fn limits() -> AnalysisLimits {
        AnalysisLimits {
            time_ms: 1_000,
            memory_bytes: 1_000_000,
            processed_path_edges: 100_000,
            witnesses_per_relation: 3,
            output_nodes: 10_000,
        }
    }

    fn ir_node(local: u64, operation: Operation) -> IrNode {
        IrNode {
            id: node(local),
            operation,
            span: None,
        }
    }

    fn edge(source: u64, target: u64, kind: EdgeKind) -> IrEdge {
        IrEdge {
            source: node(source),
            target: node(target),
            kind,
            construct: None,
            assumption: None,
        }
    }

    fn procedure(nodes: Vec<IrNode>, edges: BTreeSet<IrEdge>, exits: &[u64]) -> ProcedureIr {
        ProcedureIr {
            id: ProcedureId::new(snapshot(), 0),
            parameters: Vec::new(),
            entry: node(0),
            exits: exits.iter().map(|local| node(*local)).collect(),
            nodes: nodes
                .into_iter()
                .map(|node| (node.id.clone(), node))
                .collect(),
            edges,
        }
    }

    fn solve(procedure: &ProcedureIr, entry_facts: BTreeSet<Fact>) -> SolverOutput {
        IntraproceduralSolver::new()
            .solve(
                SolverRequest {
                    procedure,
                    entry_facts,
                    limits: limits(),
                },
                &NoopSolverControl,
            )
            .unwrap()
    }

    fn writes_for(facts: &BTreeSet<Fact>, place: &Place) -> BTreeSet<DefinitionId> {
        facts
            .iter()
            .filter_map(|fact| match fact {
                Fact::LastWrite {
                    place: candidate,
                    write,
                } if candidate == place => Some(write.clone()),
                _ => None,
            })
            .collect()
    }

    fn origins_for(facts: &BTreeSet<Fact>, place: &Place) -> BTreeSet<Source> {
        facts
            .iter()
            .filter_map(|fact| match fact {
                Fact::Origin {
                    place: candidate,
                    source,
                } if candidate == place => Some(source.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn ifds_k008_straight_line_result() {
        let source = "let x = 10; let y = x; x = 20; return y;";
        let index = index_bindings(snapshot(), "main.ts", source).unwrap();
        let selected = index
            .bindings
            .iter()
            .find(|binding| binding.name == "x")
            .unwrap()
            .id
            .clone();
        let lowered = lower_containing_procedure(source, &index, &selected).unwrap();
        let output = solve(&lowered.procedure, seed_entry_facts(&lowered.procedure));

        let x = Place::Binding(selected);
        let y = Place::Binding(
            index
                .bindings
                .iter()
                .find(|binding| binding.name == "y")
                .unwrap()
                .id
                .clone(),
        );
        let x_writes: Vec<_> = lowered
            .procedure
            .nodes
            .values()
            .filter_map(|node| match &node.operation {
                Operation::Write {
                    target, definition, ..
                } if target == &x => Some(definition.clone()),
                _ => None,
            })
            .collect();
        let y_write = lowered
            .procedure
            .nodes
            .values()
            .find_map(|node| match &node.operation {
                Operation::Write {
                    target, definition, ..
                } if target == &y => Some(definition.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(x_writes.len(), 2);

        let reads: Vec<_> = lowered
            .procedure
            .nodes
            .values()
            .filter_map(|node| match &node.operation {
                Operation::Read { source, .. } => Some((node.id.clone(), source.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(reads.len(), 2);
        assert_eq!(
            writes_for(&output.facts_at[&reads[0].0], &x),
            BTreeSet::from([x_writes[0].clone()])
        );
        assert_eq!(
            writes_for(&output.facts_at[&reads[1].0], &y),
            BTreeSet::from([y_write.clone()])
        );

        let y_origins = origins_for(&output.facts_at[&reads[1].0], &y);
        assert!(y_origins.contains(&Source::Write(y_write.clone())));
        assert!(y_origins.contains(&Source::Write(x_writes[0].clone())));
        assert!(!y_origins.contains(&Source::Write(x_writes[1].clone())));
        assert_eq!(reads[0].1, x);
        assert_eq!(reads[1].1, y);
        assert!(output.evidence.contains(&TransferEvidence::ReachingWrite {
            write: x_writes[0].clone(),
            read: reads[0].0.clone(),
            place: reads[0].1.clone(),
        }));
        assert!(output.evidence.contains(&TransferEvidence::ReachingWrite {
            write: y_write,
            read: reads[1].0.clone(),
            place: reads[1].1.clone(),
        }));
        assert_eq!(output.termination, SolverTermination::FixedPoint);
    }

    #[test]
    fn ifds_k008_join_unions_writers() {
        let x = binding(0);
        let procedure = procedure(
            vec![
                ir_node(0, Operation::Entry),
                ir_node(
                    1,
                    Operation::Write {
                        target: x.clone(),
                        sources: BTreeSet::new(),
                        definition: definition(1),
                    },
                ),
                ir_node(
                    2,
                    Operation::Write {
                        target: x.clone(),
                        sources: BTreeSet::new(),
                        definition: definition(2),
                    },
                ),
                ir_node(3, Operation::Join),
                ir_node(4, Operation::Exit),
            ],
            BTreeSet::from([
                edge(0, 1, EdgeKind::Branch { outcome: true }),
                edge(0, 2, EdgeKind::Branch { outcome: false }),
                edge(1, 3, EdgeKind::Normal),
                edge(2, 3, EdgeKind::Normal),
                edge(3, 4, EdgeKind::Normal),
            ]),
            &[4],
        );
        let output = solve(&procedure, BTreeSet::from([Fact::Zero]));

        assert_eq!(
            writes_for(&output.facts_at[&node(3)], &x),
            BTreeSet::from([definition(1), definition(2)])
        );
        assert_eq!(
            origins_for(&output.facts_at[&node(3)], &x),
            BTreeSet::from([Source::Write(definition(1)), Source::Write(definition(2))])
        );
        let incoming_writers: BTreeSet<_> = output
            .predecessors
            .iter()
            .filter_map(|predecessor| match &predecessor.destination {
                FactState {
                    node: destination,
                    fact: Fact::LastWrite { place, write },
                } if destination == &node(3) && place == &x => Some(write.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            incoming_writers,
            BTreeSet::from([definition(1), definition(2)])
        );
    }

    #[test]
    fn ifds_k008_cycle_converges() {
        let x = binding(0);
        let y = binding(1);
        let read_result = Place::Temporary(node(2));
        let procedure = procedure(
            vec![
                ir_node(0, Operation::Entry),
                ir_node(
                    1,
                    Operation::Write {
                        target: x.clone(),
                        sources: BTreeSet::new(),
                        definition: definition(1),
                    },
                ),
                ir_node(
                    2,
                    Operation::Read {
                        source: x,
                        result: read_result.clone(),
                    },
                ),
                ir_node(
                    3,
                    Operation::Write {
                        target: y.clone(),
                        sources: BTreeSet::from([read_result]),
                        definition: definition(3),
                    },
                ),
                ir_node(4, Operation::Exit),
            ],
            BTreeSet::from([
                edge(0, 1, EdgeKind::Normal),
                edge(1, 2, EdgeKind::Normal),
                edge(2, 3, EdgeKind::Normal),
                edge(3, 2, EdgeKind::LoopBack),
                edge(3, 4, EdgeKind::Normal),
            ]),
            &[4],
        );
        let output = solve(&procedure, BTreeSet::from([Fact::Zero]));
        let reference =
            evaluate_reference(&procedure, &[BTreeSet::from([Fact::Zero])], 1_000).unwrap();

        assert!(reference.complete);
        assert_eq!(output.facts_at, reference.facts_at);
        assert_eq!(
            writes_for(&output.facts_at[&node(4)], &y),
            BTreeSet::from([definition(3)])
        );
        assert!(output.stats.processed_path_edges < 100);
    }

    #[test]
    fn ifds_k008_unreachable_no_generation() {
        let reachable = binding(0);
        let disconnected = binding(1);
        let procedure = procedure(
            vec![
                ir_node(0, Operation::Entry),
                ir_node(
                    1,
                    Operation::Write {
                        target: reachable,
                        sources: BTreeSet::new(),
                        definition: definition(1),
                    },
                ),
                ir_node(2, Operation::Exit),
                ir_node(
                    3,
                    Operation::Write {
                        target: disconnected.clone(),
                        sources: BTreeSet::new(),
                        definition: definition(3),
                    },
                ),
            ],
            BTreeSet::from([edge(0, 1, EdgeKind::Normal), edge(1, 2, EdgeKind::Normal)]),
            &[2],
        );
        let output = solve(&procedure, BTreeSet::from([Fact::Zero]));

        assert!(!output.facts_at.contains_key(&node(3)));
        assert!(output.facts_at.values().all(|facts| {
            !facts.contains(&Fact::LastWrite {
                place: disconnected.clone(),
                write: definition(3),
            })
        }));
    }

    #[test]
    fn ifds_k008_worklist_order_independent() {
        let procedure = diamond_with_read();
        let solver = IntraproceduralSolver::new();
        let request = || SolverRequest {
            procedure: &procedure,
            entry_facts: BTreeSet::from([Fact::Zero]),
            limits: limits(),
        };
        let fifo = solver
            .solve_with_order(request(), &NoopSolverControl, WorklistOrder::Fifo)
            .unwrap();
        let reverse = solver
            .solve_with_order(request(), &NoopSolverControl, WorklistOrder::Reverse)
            .unwrap();

        assert_eq!(fifo.facts_at, reverse.facts_at);
        assert_eq!(fifo.predecessors, reverse.predecessors);
        assert_eq!(fifo.evidence, reverse.evidence);
        assert_eq!(fifo.unsupported, reverse.unsupported);
    }

    #[test]
    fn ifds_k008_reference_equivalence() {
        let candidates = [
            edge(0, 1, EdgeKind::Normal),
            edge(0, 2, EdgeKind::Normal),
            edge(1, 2, EdgeKind::Normal),
            edge(2, 1, EdgeKind::LoopBack),
            edge(1, 3, EdgeKind::Normal),
            edge(2, 3, EdgeKind::Normal),
        ];
        for mask in 0usize..(1usize << candidates.len()) {
            let edges = candidates
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .map(|(_, edge)| edge.clone())
                .collect();
            let x = binding(0);
            let procedure = procedure(
                vec![
                    ir_node(0, Operation::Entry),
                    ir_node(
                        1,
                        Operation::Write {
                            target: x.clone(),
                            sources: BTreeSet::new(),
                            definition: definition(1),
                        },
                    ),
                    ir_node(
                        2,
                        Operation::Write {
                            target: x.clone(),
                            sources: BTreeSet::new(),
                            definition: definition(2),
                        },
                    ),
                    ir_node(3, Operation::Exit),
                ],
                edges,
                &[3],
            );
            let output = solve(&procedure, BTreeSet::from([Fact::Zero]));
            let reference =
                evaluate_reference(&procedure, &[BTreeSet::from([Fact::Zero])], 10_000).unwrap();
            assert!(
                reference.complete,
                "reference bound exhausted for mask {mask}"
            );
            assert_eq!(output.facts_at, reference.facts_at, "graph mask {mask}");
        }
    }

    #[test]
    fn ifds_k008_unsupported_preserves_independent_facts_and_edges() {
        let x = binding(0);
        let y = binding(1);
        let procedure = procedure(
            vec![
                ir_node(0, Operation::Entry),
                ir_node(
                    1,
                    Operation::UnknownEffect {
                        effect_kind: UnknownEffectKind::Value,
                        description: "unknown".into(),
                        inputs: Vec::new(),
                        result: None,
                        definition: None,
                        affected_places: BTreeSet::from([x]),
                    },
                ),
                ir_node(2, Operation::Exit),
                ir_node(3, Operation::Exit),
            ],
            BTreeSet::from([
                edge(0, 1, EdgeKind::Normal),
                edge(1, 2, EdgeKind::Normal),
                edge(
                    1,
                    3,
                    EdgeKind::Call {
                        call_site: crate::ifds::model::CallSiteId::new(snapshot(), 1),
                    },
                ),
            ]),
            &[2, 3],
        );
        let preserved = Fact::LastWrite {
            place: y,
            write: definition(10),
        };
        let affected = Fact::LastWrite {
            place: binding(0),
            write: definition(11),
        };
        let output = solve(
            &procedure,
            BTreeSet::from([Fact::Zero, preserved.clone(), affected.clone()]),
        );

        assert!(output.facts_at[&node(2)].contains(&Fact::Zero));
        assert!(output.facts_at[&node(2)].contains(&preserved));
        assert!(!output.facts_at[&node(2)].contains(&affected));
        assert!(!output.facts_at.contains_key(&node(3)));
        assert!(output.unsupported.contains(&UnsupportedFlow::Operation {
            node: node(1),
            operation: UnsupportedTransferKind::UnknownEffect,
        }));
        assert!(
            output
                .unsupported
                .iter()
                .any(|frontier| matches!(frontier, UnsupportedFlow::Edge(_)))
        );
    }

    #[test]
    fn ifds_k008_cancellation_returns_partial_result() {
        struct CancelAfterOne;
        impl SolverControl for CancelAfterOne {
            fn checkpoint(&self, counters: &SolverCounters) -> Option<SolverCancellation> {
                (counters.processed_path_edges >= 1).then(|| SolverCancellation {
                    reason: "test cancellation".into(),
                })
            }
        }

        let procedure = procedure(
            vec![
                ir_node(0, Operation::Entry),
                ir_node(1, Operation::Join),
                ir_node(2, Operation::Exit),
            ],
            BTreeSet::from([edge(0, 1, EdgeKind::Normal), edge(1, 2, EdgeKind::Normal)]),
            &[2],
        );
        let output = IntraproceduralSolver::new()
            .solve(
                SolverRequest {
                    procedure: &procedure,
                    entry_facts: BTreeSet::from([Fact::Zero]),
                    limits: limits(),
                },
                &CancelAfterOne,
            )
            .unwrap();

        assert_eq!(output.stats.processed_path_edges, 1);
        assert!(output.facts_at[&node(1)].contains(&Fact::Zero));
        assert!(!output.facts_at.contains_key(&node(2)));
        assert_eq!(
            output.termination,
            SolverTermination::Cancelled(SolverCancellation {
                reason: "test cancellation".into(),
            })
        );
    }

    fn diamond_with_read() -> ProcedureIr {
        let x = binding(0);
        procedure(
            vec![
                ir_node(0, Operation::Entry),
                ir_node(
                    1,
                    Operation::Write {
                        target: x.clone(),
                        sources: BTreeSet::new(),
                        definition: definition(1),
                    },
                ),
                ir_node(
                    2,
                    Operation::Write {
                        target: x.clone(),
                        sources: BTreeSet::new(),
                        definition: definition(2),
                    },
                ),
                ir_node(3, Operation::Join),
                ir_node(
                    4,
                    Operation::Read {
                        source: x,
                        result: Place::Temporary(node(4)),
                    },
                ),
                ir_node(5, Operation::Exit),
            ],
            BTreeSet::from([
                edge(0, 1, EdgeKind::Branch { outcome: true }),
                edge(0, 2, EdgeKind::Branch { outcome: false }),
                edge(1, 3, EdgeKind::Normal),
                edge(2, 3, EdgeKind::Normal),
                edge(3, 4, EdgeKind::Normal),
                edge(4, 5, EdgeKind::Normal),
            ]),
            &[5],
        )
    }
}
