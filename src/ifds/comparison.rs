//! Evidence-based comparison of two independently computed flow slices.

use crate::ifds::compare::AlignmentResult;
use crate::ifds::ir::{Operation, ProcedureIr};
use crate::ifds::model::{
    AlignmentEntity, BindingId, Coverage, DeltaRecord, DiagnosticCode, EvidenceKind, FlowDelta,
    FlowEdge, FlowGraph, GroupId, LogicalNodeId, MembershipChange, NodeId, Place, RelationKind,
    RelationPresence, SnapshotSide, SourceSpan,
};
use crate::ifds::slicing::SliceOutput;
use std::collections::{BTreeMap, BTreeSet};

/// A relation's identity deliberately excludes its condition and witnesses.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RelationIdentity {
    pub source: LogicalNodeId,
    pub target: LogicalNodeId,
    pub kind: RelationKind,
    pub projection: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionComparison {
    Equivalent,
    Different,
    Unknown,
}

pub struct ComparisonInput<'a> {
    pub before: &'a SliceOutput,
    pub after: &'a SliceOutput,
    pub before_ir: &'a ProcedureIr,
    pub after_ir: &'a ProcedureIr,
    pub before_binding: Option<&'a BindingId>,
    pub after_binding: Option<&'a BindingId>,
    pub alignment: &'a AlignmentResult,
    /// Stable query identity, including the selected declaration and entry assumptions.
    pub query_key: &'a str,
    /// Local proofs may establish absence inside a globally partial slice.
    pub before_presence: &'a BTreeMap<RelationIdentity, RelationPresence>,
    pub after_presence: &'a BTreeMap<RelationIdentity, RelationPresence>,
    /// Unequal predicate text is never treated as proof of changed conditions.
    pub condition_proofs: &'a BTreeMap<RelationIdentity, ConditionComparison>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonOutput {
    pub deltas: BTreeSet<DeltaRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComparisonError {
    MissingAlignment(NodeId),
    InvalidPresenceProof(RelationIdentity),
}

#[derive(Debug, Clone)]
struct RelationEvidence {
    conditions: BTreeSet<Option<String>>,
    evidence: EvidenceKind,
}

impl RelationEvidence {
    fn condition(&self) -> Option<String> {
        if self.conditions.contains(&None) {
            return None;
        }
        if self.conditions.len() == 1 {
            return self.conditions.iter().next().cloned().flatten();
        }
        Some(
            self.conditions
                .iter()
                .filter_map(|condition| condition.as_deref())
                .map(|condition| format!("({condition})"))
                .collect::<Vec<_>>()
                .join(" || "),
        )
    }
}

pub fn compare_flow_slices(
    input: ComparisonInput<'_>,
) -> Result<ComparisonOutput, ComparisonError> {
    let before_ids = node_ids(input.alignment, SnapshotSide::Before);
    let after_ids = node_ids(input.alignment, SnapshotSide::After);
    let ambiguous: BTreeSet<_> = input
        .alignment
        .ambiguities
        .iter()
        .flat_map(|ambiguity| {
            ambiguity
                .candidates
                .iter()
                .flat_map(|candidate| [&candidate.before, &candidate.after])
                .filter_map(|entity| match entity {
                    AlignmentEntity::Node(id) => Some(id.clone()),
                    AlignmentEntity::Binding(_) => None,
                })
        })
        .collect();
    let before_nodes = graph_nodes(&input.before.graph);
    let after_nodes = graph_nodes(&input.after.graph);
    let before_relations = relations(&input.before.graph, &before_ids, &ambiguous)?;
    let after_relations = relations(&input.after.graph, &after_ids, &ambiguous)?;
    let before_snapshot = &input.before_ir.id.snapshot;
    let after_snapshot = &input.after_ir.id.snapshot;
    let group_for = |logical: LogicalNodeId| {
        let key = format!(
            "{}|{}|{}|{}|{}|{}|{}",
            input.query_key,
            before_snapshot.revision,
            before_snapshot.content_id,
            after_snapshot.revision,
            after_snapshot.content_id,
            logical.0,
            "operation"
        );
        GroupId(stable_hash(key.as_bytes()))
    };
    let mut records = BTreeMap::<FlowDelta, DeltaRecord>::new();
    let mut changed = BTreeSet::new();
    let mut explanatory = BTreeSet::new();

    for alignment in &input.alignment.nodes {
        let before_node = alignment
            .before
            .as_ref()
            .and_then(|id| before_nodes.get(id));
        let after_node = alignment.after.as_ref().and_then(|id| after_nodes.get(id));
        if before_node.is_none() && after_node.is_none() {
            continue;
        }
        let before_span = before_node.map(|node| node.span.clone());
        let after_span = after_node.map(|node| node.span.clone());
        let group = BTreeSet::from([group_for(alignment.logical)]);
        match (&alignment.before, &alignment.after) {
            (None, Some(after_id)) if after_node.is_some() => {
                changed.insert(alignment.logical);
                explanatory.insert(alignment.logical);
                insert(
                    &mut records,
                    FlowDelta::NodeAdded {
                        after: after_id.clone(),
                    },
                    group.clone(),
                    None,
                    after_span.clone(),
                    None,
                );
            }
            (Some(before_id), None) if before_node.is_some() => {
                changed.insert(alignment.logical);
                explanatory.insert(alignment.logical);
                insert(
                    &mut records,
                    FlowDelta::NodeRemoved {
                        before: before_id.clone(),
                    },
                    group.clone(),
                    before_span.clone(),
                    None,
                    None,
                );
            }
            (Some(_), Some(_)) => {
                if before_node.is_some() != after_node.is_some() {
                    explanatory.insert(alignment.logical);
                    let change = if after_node.is_some() {
                        MembershipChange::Entered
                    } else {
                        MembershipChange::Left
                    };
                    insert(
                        &mut records,
                        FlowDelta::SliceMembershipChanged {
                            logical: alignment.logical,
                            change,
                        },
                        group.clone(),
                        before_span.clone(),
                        after_span.clone(),
                        None,
                    );
                }
                let old = alignment
                    .before
                    .as_ref()
                    .and_then(|id| input.alignment.before_fingerprints.get(id));
                let new = alignment
                    .after
                    .as_ref()
                    .and_then(|id| input.alignment.after_fingerprints.get(id));
                if old.is_some()
                    && new.is_some()
                    && old != new
                    && (before_node.is_some() || after_node.is_some())
                {
                    changed.insert(alignment.logical);
                    explanatory.insert(alignment.logical);
                    insert(
                        &mut records,
                        FlowDelta::OperationChanged {
                            logical: alignment.logical,
                            before: alignment.before.as_ref().unwrap().clone(),
                            after: alignment.after.as_ref().unwrap().clone(),
                        },
                        group.clone(),
                        before_span.clone(),
                        after_span.clone(),
                        None,
                    );
                }
            }
            _ => {}
        }

        let old_write = write_presence(
            alignment.before.as_ref(),
            input.before_ir,
            input.before_binding,
        );
        let new_write = write_presence(
            alignment.after.as_ref(),
            input.after_ir,
            input.after_binding,
        );
        match (old_write, new_write) {
            (RelationPresence::Absent, RelationPresence::Present) => {
                explanatory.insert(alignment.logical);
                insert(
                    &mut records,
                    FlowDelta::WriteAdded {
                        after: alignment.after.as_ref().unwrap().clone(),
                    },
                    group.clone(),
                    before_span.clone(),
                    after_span.clone(),
                    None,
                );
            }
            (RelationPresence::Present, RelationPresence::Absent) => {
                explanatory.insert(alignment.logical);
                insert(
                    &mut records,
                    FlowDelta::WriteRemoved {
                        before: alignment.before.as_ref().unwrap().clone(),
                    },
                    group.clone(),
                    before_span.clone(),
                    after_span.clone(),
                    None,
                );
            }
            (RelationPresence::Unknown, _) | (_, RelationPresence::Unknown) => {
                insert(
                    &mut records,
                    FlowDelta::AnalysisUnknown {
                        subject: format!("write:{}", alignment.logical.0),
                        side: if old_write == RelationPresence::Unknown {
                            Some(SnapshotSide::Before)
                        } else {
                            Some(SnapshotSide::After)
                        },
                        diagnostic: DiagnosticCode::UnknownExternalEffect,
                    },
                    BTreeSet::new(),
                    before_span.clone(),
                    after_span.clone(),
                    None,
                );
            }
            _ => {}
        }
    }

    // An unchanged operation can move across another operation and alter reaching
    // definitions. Keep its identity/fingerprint, but give its consequences a group.
    explanatory.extend(reordered_nodes(
        input.alignment,
        &before_nodes,
        &after_nodes,
    ));

    let keys: BTreeSet<_> = before_relations
        .keys()
        .chain(after_relations.keys())
        .chain(input.before_presence.keys())
        .chain(input.after_presence.keys())
        .cloned()
        .collect();
    for key in keys {
        let old = before_relations.get(&key);
        let new = after_relations.get(&key);
        let old_presence = presence(
            old,
            input.before_presence.get(&key),
            input.before.extent.upstream,
            input.before.extent.downstream,
            &key,
        )?;
        let new_presence = presence(
            new,
            input.after_presence.get(&key),
            input.after.extent.upstream,
            input.after.extent.downstream,
            &key,
        )?;
        let groups = related_groups(
            &key,
            &explanatory,
            &before_relations,
            &after_relations,
            &group_for,
        );
        let (before_span, after_span) =
            relation_spans(&key, input.alignment, &before_nodes, &after_nodes);
        match (old_presence, new_presence) {
            (RelationPresence::Absent, RelationPresence::Present) => insert(
                &mut records,
                FlowDelta::FlowAdded {
                    source: key.source,
                    target: key.target,
                    relation: key.kind.clone(),
                    projection: key.projection.clone(),
                },
                groups,
                before_span,
                after_span,
                new.and_then(RelationEvidence::condition),
            ),
            (RelationPresence::Present, RelationPresence::Absent) => insert(
                &mut records,
                FlowDelta::FlowRemoved {
                    source: key.source,
                    target: key.target,
                    relation: key.kind.clone(),
                    projection: key.projection.clone(),
                },
                groups,
                before_span,
                after_span,
                old.and_then(RelationEvidence::condition),
            ),
            (RelationPresence::Present, RelationPresence::Present) => {
                if let (Some(old), Some(new)) = (old, new)
                    && old.condition() != new.condition()
                {
                    match input
                        .condition_proofs
                        .get(&key)
                        .copied()
                        .unwrap_or_else(|| {
                            compare_boolean_conditions(&old.conditions, &new.conditions)
                        }) {
                        ConditionComparison::Different => insert(
                            &mut records,
                            FlowDelta::FlowConditionChanged {
                                source: key.source,
                                target: key.target,
                                relation: key.kind.clone(),
                                projection: key.projection.clone(),
                                before: old.condition().unwrap_or_else(|| "true".into()),
                                after: new.condition().unwrap_or_else(|| "true".into()),
                            },
                            groups,
                            before_span,
                            after_span,
                            None,
                        ),
                        ConditionComparison::Unknown => insert(
                            &mut records,
                            FlowDelta::AnalysisUnknown {
                                subject: format!("condition:{key:?}"),
                                side: None,
                                diagnostic: DiagnosticCode::UnprovenPathFeasibility,
                            },
                            BTreeSet::new(),
                            before_span,
                            after_span,
                            None,
                        ),
                        ConditionComparison::Equivalent => {}
                    }
                }
            }
            (RelationPresence::Unknown, _) | (_, RelationPresence::Unknown) => {
                let side = if old_presence == RelationPresence::Unknown {
                    Some(SnapshotSide::Before)
                } else {
                    Some(SnapshotSide::After)
                };
                let diagnostic = old
                    .filter(|_| side == Some(SnapshotSide::Before))
                    .or_else(|| new.filter(|_| side == Some(SnapshotSide::After)))
                    .and_then(|e| match &e.evidence {
                        EvidenceKind::Unresolved { diagnostic } => Some(diagnostic.clone()),
                        _ => None,
                    })
                    .unwrap_or(DiagnosticCode::UnsupportedSyntax);
                insert(
                    &mut records,
                    FlowDelta::AnalysisUnknown {
                        subject: format!("presence:{key:?}"),
                        side,
                        diagnostic,
                    },
                    BTreeSet::new(),
                    before_span,
                    after_span,
                    None,
                );
            }
            _ => {}
        }
    }

    compare_sources(
        &input,
        &before_relations,
        &after_relations,
        &changed,
        &explanatory,
        &group_for,
        &mut records,
    )?;
    for ambiguity in &input.alignment.ambiguities {
        insert(
            &mut records,
            FlowDelta::AnalysisUnknown {
                subject: format!("alignment:{:?}", ambiguity.candidates),
                side: None,
                diagnostic: DiagnosticCode::AmbiguousMatch,
            },
            BTreeSet::new(),
            None,
            None,
            None,
        );
    }
    Ok(ComparisonOutput {
        deltas: records.into_values().collect(),
    })
}

fn compare_boolean_conditions(
    before: &BTreeSet<Option<String>>,
    after: &BTreeSet<Option<String>>,
) -> ConditionComparison {
    fn clauses(conditions: &BTreeSet<Option<String>>) -> Option<Vec<Vec<(String, bool)>>> {
        conditions
            .iter()
            .map(|condition| {
                condition.as_deref().map_or_else(
                    || Some(Vec::new()),
                    |text| {
                        text.split(" && ")
                            .map(|term| {
                                let (name, positive) = term
                                    .strip_prefix('!')
                                    .map_or((term, true), |name| (name, false));
                                (name
                                    .chars()
                                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                                    && name.chars().next().is_some_and(|ch| {
                                        ch.is_ascii_alphabetic() || ch == '_' || ch == '$'
                                    }))
                                .then(|| (name.to_owned(), positive))
                            })
                            .collect::<Option<Vec<_>>>()
                    },
                )
            })
            .collect()
    }
    let (Some(old), Some(new)) = (clauses(before), clauses(after)) else {
        return ConditionComparison::Unknown;
    };
    let variables: BTreeSet<_> = old
        .iter()
        .chain(&new)
        .flat_map(|clause| clause.iter().map(|term| term.0.clone()))
        .collect();
    if variables.len() > 8 {
        return ConditionComparison::Unknown;
    }
    let variables: Vec<_> = variables.into_iter().collect();
    let evaluate = |formula: &Vec<Vec<(String, bool)>>, mask: usize| {
        formula.iter().any(|clause| {
            clause.iter().all(|(name, positive)| {
                let index = variables
                    .iter()
                    .position(|variable| variable == name)
                    .unwrap();
                ((mask >> index) & 1 == 1) == *positive
            })
        })
    };
    if (0..(1 << variables.len())).all(|mask| evaluate(&old, mask) == evaluate(&new, mask)) {
        ConditionComparison::Equivalent
    } else {
        ConditionComparison::Different
    }
}

fn node_ids(alignment: &AlignmentResult, side: SnapshotSide) -> BTreeMap<NodeId, LogicalNodeId> {
    alignment
        .nodes
        .iter()
        .filter_map(|a| {
            match side {
                SnapshotSide::Before => a.before.as_ref(),
                SnapshotSide::After => a.after.as_ref(),
            }
            .map(|id| (id.clone(), a.logical))
        })
        .collect()
}

fn graph_nodes(graph: &FlowGraph) -> BTreeMap<NodeId, &crate::ifds::model::FlowNode> {
    graph
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect()
}

fn relations(
    graph: &FlowGraph,
    ids: &BTreeMap<NodeId, LogicalNodeId>,
    ambiguous: &BTreeSet<NodeId>,
) -> Result<BTreeMap<RelationIdentity, RelationEvidence>, ComparisonError> {
    let mut result = BTreeMap::new();
    for edge in &graph.edges {
        if ambiguous.contains(&edge.source) || ambiguous.contains(&edge.target) {
            continue;
        }
        let key = relation_key(edge, ids)?;
        result
            .entry(key)
            .and_modify(|value: &mut RelationEvidence| {
                value.conditions.insert(edge.condition.clone());
                if matches!(value.evidence, EvidenceKind::Unresolved { .. })
                    && !matches!(edge.evidence, EvidenceKind::Unresolved { .. })
                {
                    value.evidence = edge.evidence.clone();
                }
            })
            .or_insert(RelationEvidence {
                conditions: BTreeSet::from([edge.condition.clone()]),
                evidence: edge.evidence.clone(),
            });
    }
    Ok(result)
}

fn relation_key(
    edge: &FlowEdge,
    ids: &BTreeMap<NodeId, LogicalNodeId>,
) -> Result<RelationIdentity, ComparisonError> {
    Ok(RelationIdentity {
        source: *ids
            .get(&edge.source)
            .ok_or_else(|| ComparisonError::MissingAlignment(edge.source.clone()))?,
        target: *ids
            .get(&edge.target)
            .ok_or_else(|| ComparisonError::MissingAlignment(edge.target.clone()))?,
        kind: edge.relation.clone(),
        projection: edge.projection.clone(),
    })
}

fn presence(
    edge: Option<&RelationEvidence>,
    proof: Option<&RelationPresence>,
    upstream: Coverage,
    downstream: Coverage,
    key: &RelationIdentity,
) -> Result<RelationPresence, ComparisonError> {
    let observed = edge.map(|e| match e.evidence {
        EvidenceKind::Unresolved { .. } => RelationPresence::Unknown,
        _ => RelationPresence::Present,
    });
    if let (Some(proof), Some(observed)) = (proof, observed)
        && ((*proof == RelationPresence::Absent && observed == RelationPresence::Present)
            || (*proof == RelationPresence::Present && observed == RelationPresence::Unknown))
    {
        return Err(ComparisonError::InvalidPresenceProof(key.clone()));
    }
    Ok(proof.copied().or(observed).unwrap_or(
        if upstream == Coverage::Complete && downstream == Coverage::Complete {
            RelationPresence::Absent
        } else {
            RelationPresence::Unknown
        },
    ))
}

fn write_presence(
    id: Option<&NodeId>,
    ir: &ProcedureIr,
    selected: Option<&BindingId>,
) -> RelationPresence {
    let Some(id) = id else {
        return RelationPresence::Absent;
    };
    let Some(node) = ir.nodes.get(id) else {
        return RelationPresence::Unknown;
    };
    match &node.operation {
        Operation::Write {
            target: Place::Binding(binding),
            ..
        } if Some(binding) == selected => RelationPresence::Present,
        Operation::UnknownEffect {
            affected_places, ..
        } if selected
            .is_some_and(|binding| affected_places.contains(&Place::Binding(binding.clone()))) =>
        {
            RelationPresence::Unknown
        }
        Operation::Call { .. } => RelationPresence::Unknown,
        _ => RelationPresence::Absent,
    }
}

fn insert(
    records: &mut BTreeMap<FlowDelta, DeltaRecord>,
    delta: FlowDelta,
    groups: BTreeSet<GroupId>,
    before_span: Option<SourceSpan>,
    after_span: Option<SourceSpan>,
    condition: Option<String>,
) {
    records
        .entry(delta.clone())
        .and_modify(|existing| existing.groups.extend(groups.clone()))
        .or_insert(DeltaRecord {
            delta,
            groups,
            before_span,
            after_span,
            condition,
        });
}

fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn reordered_nodes(
    alignment: &AlignmentResult,
    before: &BTreeMap<NodeId, &crate::ifds::model::FlowNode>,
    after: &BTreeMap<NodeId, &crate::ifds::model::FlowNode>,
) -> BTreeSet<LogicalNodeId> {
    let paired: Vec<_> = alignment
        .nodes
        .iter()
        .filter_map(|a| {
            Some((
                a.logical,
                before.get(a.before.as_ref()?)?.span.byte_start,
                after.get(a.after.as_ref()?)?.span.byte_start,
            ))
        })
        .collect();
    let mut result = BTreeSet::new();
    for (i, left) in paired.iter().enumerate() {
        for right in paired.iter().skip(i + 1) {
            if (left.1 < right.1 && left.2 > right.2) || (left.1 > right.1 && left.2 < right.2) {
                result.insert(left.0);
                result.insert(right.0);
            }
        }
    }
    result
}

fn related_groups(
    key: &RelationIdentity,
    explanatory: &BTreeSet<LogicalNodeId>,
    before: &BTreeMap<RelationIdentity, RelationEvidence>,
    after: &BTreeMap<RelationIdentity, RelationEvidence>,
    group_for: &impl Fn(LogicalNodeId) -> GroupId,
) -> BTreeSet<GroupId> {
    let mut contributors = BTreeSet::new();
    for node in explanatory {
        if *node == key.source
            || *node == key.target
            || reaches(*node, key.source, before)
            || reaches(*node, key.source, after)
        {
            contributors.insert(group_for(*node));
        }
    }
    contributors
}

fn reaches(
    start: LogicalNodeId,
    target: LogicalNodeId,
    relations: &BTreeMap<RelationIdentity, RelationEvidence>,
) -> bool {
    let mut pending = vec![start];
    let mut visited = BTreeSet::new();
    while let Some(node) = pending.pop() {
        if !visited.insert(node) {
            continue;
        }
        if node == target {
            return true;
        }
        pending.extend(
            relations
                .keys()
                .filter(|key| key.source == node && value_relation(&key.kind))
                .map(|key| key.target),
        );
    }
    false
}

fn value_relation(kind: &RelationKind) -> bool {
    matches!(
        kind,
        RelationKind::Reaches
            | RelationKind::ValueDependency
            | RelationKind::Argument
            | RelationKind::Return
    )
}

fn relation_spans(
    key: &RelationIdentity,
    alignment: &AlignmentResult,
    before: &BTreeMap<NodeId, &crate::ifds::model::FlowNode>,
    after: &BTreeMap<NodeId, &crate::ifds::model::FlowNode>,
) -> (Option<SourceSpan>, Option<SourceSpan>) {
    let target = alignment.nodes.iter().find(|a| a.logical == key.target);
    (
        target
            .and_then(|a| a.before.as_ref())
            .and_then(|id| before.get(id))
            .map(|n| n.span.clone()),
        target
            .and_then(|a| a.after.as_ref())
            .and_then(|id| after.get(id))
            .map(|n| n.span.clone()),
    )
}

fn compare_sources(
    input: &ComparisonInput<'_>,
    before_relations: &BTreeMap<RelationIdentity, RelationEvidence>,
    after_relations: &BTreeMap<RelationIdentity, RelationEvidence>,
    changed: &BTreeSet<LogicalNodeId>,
    explanatory: &BTreeSet<LogicalNodeId>,
    group_for: &impl Fn(LogicalNodeId) -> GroupId,
    records: &mut BTreeMap<FlowDelta, DeltaRecord>,
) -> Result<(), ComparisonError> {
    let before_ids = node_ids(input.alignment, SnapshotSide::Before);
    let after_ids = node_ids(input.alignment, SnapshotSide::After);
    let old_inputs = inputs(before_relations);
    let new_inputs = inputs(after_relations);
    let targets: BTreeSet<_> = old_inputs
        .keys()
        .chain(new_inputs.keys())
        .cloned()
        .collect();
    let old_nodes = graph_nodes(&input.before.graph);
    let new_nodes = graph_nodes(&input.after.graph);
    for (consumer, projection) in targets {
        let paired = input.alignment.nodes.iter().any(|a| {
            a.logical == consumer
                && a.before
                    .as_ref()
                    .is_some_and(|id| old_nodes.contains_key(id))
                && a.after
                    .as_ref()
                    .is_some_and(|id| new_nodes.contains_key(id))
        });
        if !paired {
            continue;
        }
        let old_sources = old_inputs
            .get(&(consumer, projection.clone()))
            .cloned()
            .unwrap_or_default();
        let new_sources = new_inputs
            .get(&(consumer, projection.clone()))
            .cloned()
            .unwrap_or_default();
        let old_bindings =
            source_bindings(&old_sources, &before_ids, input.before_ir, input.alignment);
        let new_bindings =
            source_bindings(&new_sources, &after_ids, input.after_ir, input.alignment);
        let old_changed = changed_ancestors(&old_sources, changed, before_relations);
        let new_changed = changed_ancestors(&new_sources, changed, after_relations);
        let changed_operations: BTreeSet<_> = old_changed.union(&new_changed).copied().collect();
        if old_sources == new_sources
            && old_bindings == new_bindings
            && changed_operations.is_empty()
        {
            continue;
        }
        let keys: BTreeSet<_> = before_relations
            .keys()
            .chain(after_relations.keys())
            .filter(|k| {
                k.target == consumer
                    && k.projection.as_deref() == Some(&projection)
                    && value_relation(&k.kind)
            })
            .cloned()
            .collect();
        if keys.iter().any(|key| {
            let old = presence(
                before_relations.get(key),
                input.before_presence.get(key),
                input.before.extent.upstream,
                input.before.extent.downstream,
                key,
            );
            let new = presence(
                after_relations.get(key),
                input.after_presence.get(key),
                input.after.extent.upstream,
                input.after.extent.downstream,
                key,
            );
            !matches!(
                (old, new),
                (
                    Ok(RelationPresence::Present | RelationPresence::Absent),
                    Ok(RelationPresence::Present | RelationPresence::Absent)
                )
            )
        }) || input.before.extent.upstream != Coverage::Complete
            || input.after.extent.upstream != Coverage::Complete
        {
            insert(
                records,
                FlowDelta::AnalysisUnknown {
                    subject: format!("sources:{}:{projection}", consumer.0),
                    side: None,
                    diagnostic: DiagnosticCode::UnsupportedSyntax,
                },
                BTreeSet::new(),
                None,
                None,
                None,
            );
            continue;
        }
        let mut groups = BTreeSet::new();
        for node in explanatory {
            if changed_operations.contains(node)
                || old_sources.contains(node)
                || new_sources.contains(node)
                || old_sources
                    .iter()
                    .any(|s| reaches(*node, *s, before_relations))
                || new_sources
                    .iter()
                    .any(|s| reaches(*node, *s, after_relations))
            {
                groups.insert(group_for(*node));
            }
        }
        let key = RelationIdentity {
            source: LogicalNodeId(0),
            target: consumer,
            kind: RelationKind::ValueDependency,
            projection: Some(projection.clone()),
        };
        let (before_span, after_span) =
            relation_spans(&key, input.alignment, &old_nodes, &new_nodes);
        insert(
            records,
            FlowDelta::ValueSourceChanged {
                consumer,
                projection,
                before_sources: old_sources,
                after_sources: new_sources,
                before_bindings: old_bindings,
                after_bindings: new_bindings,
                changed_operations,
            },
            groups,
            before_span,
            after_span,
            None,
        );
    }
    Ok(())
}

fn source_bindings(
    sources: &BTreeSet<LogicalNodeId>,
    ids: &BTreeMap<NodeId, LogicalNodeId>,
    ir: &ProcedureIr,
    alignment: &AlignmentResult,
) -> BTreeSet<crate::ifds::model::LogicalBindingId> {
    let mut result = BTreeSet::new();
    for source in sources {
        for (id, _) in ids.iter().filter(|(_, logical)| *logical == source) {
            if let Some(crate::ifds::ir::IrNode {
                operation:
                    Operation::Read {
                        source: Place::Binding(binding),
                        ..
                    },
                ..
            }) = ir.nodes.get(id)
                && let Some(logical) = alignment.bindings.iter().find(|a| {
                    a.before.as_ref() == Some(binding) || a.after.as_ref() == Some(binding)
                })
            {
                result.insert(logical.logical);
            }
        }
    }
    result
}

fn inputs(
    relations: &BTreeMap<RelationIdentity, RelationEvidence>,
) -> BTreeMap<(LogicalNodeId, String), BTreeSet<LogicalNodeId>> {
    let mut result = BTreeMap::<_, BTreeSet<_>>::new();
    for key in relations.keys().filter(|key| value_relation(&key.kind)) {
        result
            .entry((
                key.target,
                key.projection.clone().unwrap_or_else(|| "value".into()),
            ))
            .or_default()
            .insert(key.source);
    }
    result
}

fn changed_ancestors(
    sources: &BTreeSet<LogicalNodeId>,
    changed: &BTreeSet<LogicalNodeId>,
    relations: &BTreeMap<RelationIdentity, RelationEvidence>,
) -> BTreeSet<LogicalNodeId> {
    changed
        .iter()
        .filter(|node| {
            sources
                .iter()
                .any(|source| reaches(**node, *source, relations))
        })
        .copied()
        .collect()
}

#[cfg(test)]
mod tests;
