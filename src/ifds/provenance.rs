//! Finite reaching-definition and origin transfer functions.

use crate::ifds::ir::{Operation, ProcedureIr};
use crate::ifds::model::{DefinitionId, Fact, NodeId, Place, Source};
use crate::ifds::solver::FlowFunction;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedTransferKind {
    Call,
    UnknownEffect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferOutcome {
    Supported(BTreeSet<Fact>),
    Unsupported {
        preserved: BTreeSet<Fact>,
        operation: UnsupportedTransferKind,
    },
}

impl TransferOutcome {
    pub fn facts(&self) -> &BTreeSet<Fact> {
        match self {
            Self::Supported(facts) => facts,
            Self::Unsupported { preserved, .. } => preserved,
        }
    }

    pub fn into_facts(self) -> BTreeSet<Fact> {
        match self {
            Self::Supported(facts) => facts,
            Self::Unsupported { preserved, .. } => preserved,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TransferEvidence {
    ReachingWrite {
        write: DefinitionId,
        read: NodeId,
        place: Place,
    },
    OriginCopied {
        operation: NodeId,
        source_place: Place,
        target_place: Place,
        source: Source,
    },
}

pub struct ProcedureFlowFunctions<'a> {
    procedure: &'a ProcedureIr,
}

impl<'a> ProcedureFlowFunctions<'a> {
    pub fn new(procedure: &'a ProcedureIr) -> Self {
        Self { procedure }
    }

    pub fn transfer_all(&self, node: &NodeId, facts: &BTreeSet<Fact>) -> TransferOutcome {
        let operation = &self.procedure.nodes[node].operation;
        let unsupported = unsupported_kind(operation);
        let transferred = facts
            .iter()
            .flat_map(|fact| transfer_operation(operation, fact).into_facts())
            .collect();
        match unsupported {
            Some(operation) => TransferOutcome::Unsupported {
                preserved: transferred,
                operation,
            },
            None => TransferOutcome::Supported(transferred),
        }
    }

    pub fn evidence(&self, node: &NodeId, fact: &Fact) -> BTreeSet<TransferEvidence> {
        transfer_evidence(node, &self.procedure.nodes[node].operation, fact)
    }
}

impl FlowFunction for ProcedureFlowFunctions<'_> {
    fn transfer(&self, node: &NodeId, fact: &Fact) -> TransferOutcome {
        transfer_operation(&self.procedure.nodes[node].operation, fact)
    }

    fn evidence(&self, node: &NodeId, fact: &Fact) -> BTreeSet<TransferEvidence> {
        ProcedureFlowFunctions::evidence(self, node, fact)
    }
}

pub fn seed_entry_facts(procedure: &ProcedureIr) -> BTreeSet<Fact> {
    let mut facts = BTreeSet::from([Fact::Zero]);
    for parameter in &procedure.parameters {
        let place = Place::Binding(parameter.binding.clone());
        facts.insert(Fact::LastWrite {
            place: place.clone(),
            write: parameter.entry_definition.clone(),
        });
        facts.insert(Fact::Origin {
            place: place.clone(),
            source: Source::ParameterEntry(parameter.entry_definition.clone()),
        });
        facts.insert(Fact::Origin {
            place,
            source: Source::FunctionInput(parameter.binding.clone()),
        });
    }
    facts
}

pub fn transfer_operation(operation: &Operation, fact: &Fact) -> TransferOutcome {
    match operation {
        Operation::Read { source, result } => {
            TransferOutcome::Supported(transfer_read(source, result, fact))
        }
        Operation::Write {
            target,
            sources,
            definition,
        } => TransferOutcome::Supported(transfer_definition(
            target,
            sources.iter(),
            definition,
            fact,
        )),
        Operation::Literal {
            definition, result, ..
        } => TransferOutcome::Supported(transfer_definition(
            result,
            std::iter::empty(),
            definition,
            fact,
        )),
        Operation::Compute {
            definition,
            inputs,
            result,
            ..
        } => TransferOutcome::Supported(transfer_definition(
            result,
            inputs.iter().map(|input| &input.place),
            definition,
            fact,
        )),
        Operation::Call { result, .. } => TransferOutcome::Unsupported {
            preserved: preserve_unaffected(fact, result.iter(), std::iter::empty()),
            operation: UnsupportedTransferKind::Call,
        },
        Operation::UnknownEffect {
            result,
            affected_places,
            ..
        } => TransferOutcome::Unsupported {
            preserved: preserve_unaffected(fact, result.iter(), affected_places.iter()),
            operation: UnsupportedTransferKind::UnknownEffect,
        },
        Operation::Entry
        | Operation::Branch { .. }
        | Operation::Join
        | Operation::ReturnSite { .. }
        | Operation::Return { .. }
        | Operation::Exit => TransferOutcome::Supported(BTreeSet::from([fact.clone()])),
    }
}

fn transfer_read(source: &Place, result: &Place, fact: &Fact) -> BTreeSet<Fact> {
    let mut outgoing = BTreeSet::new();
    if fact_place(fact) != Some(result) {
        outgoing.insert(fact.clone());
    }
    if let Fact::Origin {
        place,
        source: origin,
    } = fact
        && place == source
    {
        outgoing.insert(Fact::Origin {
            place: result.clone(),
            source: origin.clone(),
        });
    }
    outgoing
}

fn transfer_definition<'a>(
    target: &Place,
    sources: impl Iterator<Item = &'a Place>,
    definition: &DefinitionId,
    fact: &Fact,
) -> BTreeSet<Fact> {
    let sources: BTreeSet<_> = sources.collect();
    let mut outgoing = BTreeSet::new();
    if fact == &Fact::Zero {
        outgoing.insert(Fact::Zero);
        outgoing.insert(Fact::LastWrite {
            place: target.clone(),
            write: definition.clone(),
        });
        outgoing.insert(Fact::Origin {
            place: target.clone(),
            source: Source::Write(definition.clone()),
        });
        return outgoing;
    }
    if fact_place(fact) != Some(target) {
        outgoing.insert(fact.clone());
    }
    if let Fact::Origin { place, source } = fact
        && sources.contains(place)
    {
        outgoing.insert(Fact::Origin {
            place: target.clone(),
            source: source.clone(),
        });
    }
    outgoing
}

fn preserve_unaffected<'a>(
    fact: &Fact,
    results: impl Iterator<Item = &'a Place>,
    affected: impl Iterator<Item = &'a Place>,
) -> BTreeSet<Fact> {
    let killed: BTreeSet<_> = results.chain(affected).collect();
    if fact_place(fact).is_some_and(|place| killed.contains(place)) {
        BTreeSet::new()
    } else {
        BTreeSet::from([fact.clone()])
    }
}

fn fact_place(fact: &Fact) -> Option<&Place> {
    match fact {
        Fact::LastWrite { place, .. } | Fact::Origin { place, .. } => Some(place),
        Fact::Zero => None,
    }
}

fn unsupported_kind(operation: &Operation) -> Option<UnsupportedTransferKind> {
    match operation {
        Operation::Call { .. } => Some(UnsupportedTransferKind::Call),
        Operation::UnknownEffect { .. } => Some(UnsupportedTransferKind::UnknownEffect),
        _ => None,
    }
}

fn transfer_evidence(
    node: &NodeId,
    operation: &Operation,
    fact: &Fact,
) -> BTreeSet<TransferEvidence> {
    let mut evidence = BTreeSet::new();
    match (operation, fact) {
        (Operation::Read { source, .. }, Fact::LastWrite { place, write }) if place == source => {
            evidence.insert(TransferEvidence::ReachingWrite {
                write: write.clone(),
                read: node.clone(),
                place: source.clone(),
            });
        }
        (
            Operation::Read { source, result },
            Fact::Origin {
                place,
                source: origin,
            },
        ) if place == source => {
            evidence.insert(TransferEvidence::OriginCopied {
                operation: node.clone(),
                source_place: source.clone(),
                target_place: result.clone(),
                source: origin.clone(),
            });
        }
        (
            Operation::Write {
                target, sources, ..
            },
            Fact::Origin {
                place,
                source: origin,
            },
        ) if sources.contains(place) => {
            evidence.insert(TransferEvidence::OriginCopied {
                operation: node.clone(),
                source_place: place.clone(),
                target_place: target.clone(),
                source: origin.clone(),
            });
        }
        (
            Operation::Compute { inputs, result, .. },
            Fact::Origin {
                place,
                source: origin,
            },
        ) if inputs.iter().any(|input| &input.place == place) => {
            evidence.insert(TransferEvidence::OriginCopied {
                operation: node.clone(),
                source_place: place.clone(),
                target_place: result.clone(),
                source: origin.clone(),
            });
        }
        _ => {}
    }
    evidence
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ifds::fixtures::is_distributive;
    use crate::ifds::ir::{
        ComputeInput, ComputeInputRole, IrNode, LiteralKind, PrimitiveOperator, ProcedureParameter,
    };
    use crate::ifds::model::{BindingId, ProcedureId, SnapshotId, SnapshotSide, SourceSpan};
    use std::collections::BTreeMap;

    fn snapshot() -> SnapshotId {
        SnapshotId {
            side: SnapshotSide::Before,
            revision: "k007".into(),
            content_id: "k007-content".into(),
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

    fn temporary(local: u64) -> Place {
        Place::Temporary(node(local))
    }

    fn span() -> SourceSpan {
        SourceSpan {
            path: "test.ts".into(),
            byte_start: 0,
            byte_end: 1,
            start_line: 1,
            end_line: 1,
        }
    }

    fn apply(operation: &Operation, facts: &BTreeSet<Fact>) -> BTreeSet<Fact> {
        facts
            .iter()
            .flat_map(|fact| transfer_operation(operation, fact).into_facts())
            .collect()
    }

    fn write(target: Place, sources: impl IntoIterator<Item = Place>, local: u64) -> Operation {
        Operation::Write {
            target,
            sources: sources.into_iter().collect(),
            definition: definition(local),
        }
    }

    #[test]
    fn ifds_k007_zero_generates_literal() {
        let result = temporary(1);
        let operation = Operation::Literal {
            definition: definition(10),
            literal_kind: LiteralKind::Number,
            raw: "10".into(),
            cooked: None,
            result: result.clone(),
        };
        assert_eq!(
            apply(&operation, &BTreeSet::from([Fact::Zero])),
            BTreeSet::from([
                Fact::Zero,
                Fact::LastWrite {
                    place: result.clone(),
                    write: definition(10),
                },
                Fact::Origin {
                    place: result,
                    source: Source::Write(definition(10)),
                },
            ])
        );
        assert!(apply(&operation, &BTreeSet::new()).is_empty());
    }

    #[test]
    fn ifds_k007_overwrite_kills_one_place() {
        let x = binding(1);
        let y = binding(2);
        let incoming = BTreeSet::from([
            Fact::Zero,
            Fact::LastWrite {
                place: x.clone(),
                write: definition(1),
            },
            Fact::Origin {
                place: x.clone(),
                source: Source::Write(definition(1)),
            },
            Fact::LastWrite {
                place: y.clone(),
                write: definition(2),
            },
            Fact::Origin {
                place: y.clone(),
                source: Source::Write(definition(2)),
            },
        ]);
        let outgoing = apply(&write(x.clone(), [], 3), &incoming);
        assert!(outgoing.contains(&Fact::LastWrite {
            place: x.clone(),
            write: definition(3),
        }));
        assert!(outgoing.contains(&Fact::Origin {
            place: x.clone(),
            source: Source::Write(definition(3)),
        }));
        assert!(!outgoing.contains(&Fact::LastWrite {
            place: x,
            write: definition(1),
        }));
        assert!(outgoing.contains(&Fact::LastWrite {
            place: y.clone(),
            write: definition(2),
        }));
        assert!(outgoing.contains(&Fact::Origin {
            place: y,
            source: Source::Write(definition(2)),
        }));
    }

    #[test]
    fn ifds_k007_copy_survives_overwrite() {
        let x = binding(1);
        let y = binding(2);
        let read_result = temporary(20);
        let mut facts = BTreeSet::from([Fact::Zero]);
        facts = apply(&write(x.clone(), [], 10), &facts);
        facts = apply(
            &Operation::Read {
                source: x.clone(),
                result: read_result.clone(),
            },
            &facts,
        );
        facts = apply(&write(y.clone(), [read_result], 11), &facts);
        facts = apply(&write(x, [], 12), &facts);

        assert!(facts.contains(&Fact::LastWrite {
            place: y.clone(),
            write: definition(11),
        }));
        assert!(facts.contains(&Fact::Origin {
            place: y.clone(),
            source: Source::Write(definition(10)),
        }));
        assert!(facts.contains(&Fact::Origin {
            place: y.clone(),
            source: Source::Write(definition(11)),
        }));
        assert!(!facts.contains(&Fact::Origin {
            place: y,
            source: Source::Write(definition(12)),
        }));
    }

    #[test]
    fn ifds_k007_self_assignment_origins() {
        let x = binding(1);
        let old_x = temporary(20);
        let one = temporary(21);
        let sum = temporary(22);
        let mut facts = apply(&write(x.clone(), [], 10), &BTreeSet::from([Fact::Zero]));
        facts = apply(
            &Operation::Read {
                source: x.clone(),
                result: old_x.clone(),
            },
            &facts,
        );
        facts = apply(
            &Operation::Literal {
                definition: definition(11),
                literal_kind: LiteralKind::Number,
                raw: "1".into(),
                cooked: None,
                result: one.clone(),
            },
            &facts,
        );
        facts = apply(
            &Operation::Compute {
                definition: definition(12),
                inputs: vec![
                    ComputeInput {
                        place: old_x,
                        role: ComputeInputRole::Operand { index: 0 },
                        span: span(),
                    },
                    ComputeInput {
                        place: one,
                        role: ComputeInputRole::Operand { index: 1 },
                        span: span(),
                    },
                ],
                result: sum.clone(),
                operator: PrimitiveOperator::Add,
            },
            &facts,
        );
        facts = apply(&write(x.clone(), [sum], 13), &facts);

        for origin in [10, 11, 12, 13] {
            assert!(facts.contains(&Fact::Origin {
                place: x.clone(),
                source: Source::Write(definition(origin)),
            }));
        }
        assert!(facts.contains(&Fact::LastWrite {
            place: x,
            write: definition(13),
        }));
    }

    #[test]
    fn ifds_k007_parameter_seeding() {
        let entry = node(1);
        let exit = node(2);
        let first = BindingId::new(snapshot(), 1);
        let second = BindingId::new(snapshot(), 2);
        let procedure = ProcedureIr {
            id: ProcedureId::new(snapshot(), 0),
            parameters: vec![
                ProcedureParameter {
                    index: 0,
                    binding: first.clone(),
                    entry_definition: definition(100),
                },
                ProcedureParameter {
                    index: 1,
                    binding: second.clone(),
                    entry_definition: definition(101),
                },
            ],
            entry: entry.clone(),
            exits: BTreeSet::from([exit.clone()]),
            nodes: BTreeMap::from([
                (
                    entry.clone(),
                    IrNode {
                        id: entry,
                        operation: Operation::Entry,
                        span: None,
                    },
                ),
                (
                    exit.clone(),
                    IrNode {
                        id: exit,
                        operation: Operation::Exit,
                        span: None,
                    },
                ),
            ]),
            edges: BTreeSet::new(),
        };
        procedure.validate().unwrap();
        let facts = seed_entry_facts(&procedure);
        for (binding, definition) in [(&first, 100), (&second, 101)] {
            let place = Place::Binding(binding.clone());
            assert!(facts.contains(&Fact::LastWrite {
                place: place.clone(),
                write: self::definition(definition),
            }));
            assert!(facts.contains(&Fact::Origin {
                place: place.clone(),
                source: Source::ParameterEntry(self::definition(definition)),
            }));
            assert!(facts.contains(&Fact::Origin {
                place,
                source: Source::FunctionInput(binding.clone()),
            }));
        }
        assert!(!facts.contains(&Fact::Origin {
            place: Place::Binding(first),
            source: Source::FunctionInput(second),
        }));
    }

    #[test]
    fn ifds_k007_distributivity() {
        let source = binding(1);
        let target = binding(2);
        let result = temporary(20);
        let domain = BTreeSet::from([
            Fact::Zero,
            Fact::LastWrite {
                place: source.clone(),
                write: definition(1),
            },
            Fact::Origin {
                place: source.clone(),
                source: Source::Write(definition(1)),
            },
            Fact::Origin {
                place: target.clone(),
                source: Source::Write(definition(2)),
            },
        ]);
        let operations = [
            Operation::Read {
                source: source.clone(),
                result: result.clone(),
            },
            write(target, [source.clone()], 3),
            Operation::Literal {
                definition: definition(4),
                literal_kind: LiteralKind::Number,
                raw: "1".into(),
                cooked: None,
                result: result.clone(),
            },
            Operation::Compute {
                definition: definition(5),
                inputs: vec![ComputeInput {
                    place: source,
                    role: ComputeInputRole::Operand { index: 0 },
                    span: span(),
                }],
                result,
                operator: PrimitiveOperator::UnaryPlus,
            },
            Operation::Join,
        ];
        for operation in operations {
            assert!(
                is_distributive(&domain, |facts| apply(&operation, facts)),
                "operation was not distributive: {operation:?}"
            );
        }
    }

    #[test]
    fn ifds_k007_finite_static_ids() {
        let operation = write(binding(1), [], 10);
        let once = apply(&operation, &BTreeSet::from([Fact::Zero]));
        let twice = apply(&operation, &once);
        assert_eq!(once, twice);
        assert_eq!(
            twice
                .iter()
                .filter(|fact| matches!(fact, Fact::LastWrite { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn ifds_k007_read_evidence_is_separate_from_facts() {
        let source = binding(1);
        let result = temporary(20);
        let operation = Operation::Read {
            source: source.clone(),
            result: result.clone(),
        };
        let fact = Fact::LastWrite {
            place: source.clone(),
            write: definition(10),
        };
        let transferred = transfer_operation(&operation, &fact).into_facts();
        assert!(!transferred.contains(&Fact::LastWrite {
            place: result,
            write: definition(10),
        }));
        assert_eq!(
            transfer_evidence(&node(30), &operation, &fact),
            BTreeSet::from([TransferEvidence::ReachingWrite {
                write: definition(10),
                read: node(30),
                place: source,
            }])
        );
    }

    #[test]
    fn ifds_k007_unsupported_transfer_is_explicit() {
        let source = binding(1);
        let result = temporary(20);
        let fact = Fact::Origin {
            place: source.clone(),
            source: Source::Write(definition(10)),
        };
        let operation = Operation::Call {
            call_site: crate::ifds::model::CallSiteId::new(snapshot(), 1),
            arguments: vec![source],
            result: Some(result),
        };
        assert_eq!(
            transfer_operation(&operation, &fact),
            TransferOutcome::Unsupported {
                preserved: BTreeSet::from([fact]),
                operation: UnsupportedTransferKind::Call,
            }
        );
    }
}
