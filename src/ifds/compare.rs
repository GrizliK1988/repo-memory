//! Deterministic cross-revision identity alignment.
//!
//! Alignment is intentionally separate from delta classification.  This module
//! establishes which snapshot-local declarations and operations may be compared;
//! `K012` consumes that result to decide what changed in the flow graph.

use crate::ifds::ir::{ComputeInputRole, Operation, ProcedureIr};
use crate::ifds::model::{
    Alignment, AlignmentAmbiguity, AlignmentCandidate, AlignmentEntity, BindingAlignment,
    BindingId, Diagnostic, DiagnosticCode, FileChangeKind, LogicalBindingId, LogicalNodeId, NodeId,
    Place, RepositoryDiff, SourceSpan,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlignableBindingRole {
    Local,
    Parameter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlignableScopeRole {
    Program,
    Function,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AlignableBinding {
    pub id: BindingId,
    pub name: String,
    pub role: AlignableBindingRole,
    pub declaration: SourceSpan,
    pub enclosing_symbol: Option<String>,
    pub scope_path: Vec<AlignableScopeRole>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignmentResult {
    pub bindings: BTreeSet<BindingAlignment>,
    pub nodes: BTreeSet<Alignment>,
    pub ambiguities: BTreeSet<AlignmentAmbiguity>,
    pub diagnostics: BTreeSet<Diagnostic>,
    pub before_fingerprints: BTreeMap<NodeId, String>,
    pub after_fingerprints: BTreeMap<NodeId, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlignmentError {
    InvalidSnapshots(String),
    MissingSelectedBinding(BindingId),
    MissingCounterpart(BindingId),
    IncompatibleCounterpart(String),
}

impl fmt::Display for AlignmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSnapshots(reason) => write!(f, "invalid alignment snapshots: {reason}"),
            Self::MissingSelectedBinding(binding) => {
                write!(
                    f,
                    "selected binding is absent from alignment input: {binding:?}"
                )
            }
            Self::MissingCounterpart(binding) => {
                write!(
                    f,
                    "explicit counterpart is absent from alignment input: {binding:?}"
                )
            }
            Self::IncompatibleCounterpart(reason) => {
                write!(f, "incompatible explicit counterpart: {reason}")
            }
        }
    }
}

impl std::error::Error for AlignmentError {}

/// Align two already-lowered versions of one containing procedure.
///
/// `selected` may belong to either side.  Supplying `explicit_counterpart`
/// validates and fixes that declaration pairing before automatic alignment.
pub fn align_procedures(
    before: &ProcedureIr,
    after: &ProcedureIr,
    before_bindings: &[AlignableBinding],
    after_bindings: &[AlignableBinding],
    diff: &RepositoryDiff,
    selected: &BindingId,
    explicit_counterpart: Option<&BindingId>,
) -> Result<AlignmentResult, AlignmentError> {
    if before.id.snapshot.side != crate::ifds::model::SnapshotSide::Before
        || after.id.snapshot.side != crate::ifds::model::SnapshotSide::After
    {
        return Err(AlignmentError::InvalidSnapshots(
            "procedures must be ordered before, then after".into(),
        ));
    }

    let before_by_id: BTreeMap<_, _> = before_bindings
        .iter()
        .map(|binding| (binding.id.clone(), binding))
        .collect();
    let after_by_id: BTreeMap<_, _> = after_bindings
        .iter()
        .map(|binding| (binding.id.clone(), binding))
        .collect();
    let selected_before = before_by_id.get(selected).copied();
    let selected_after = after_by_id.get(selected).copied();
    if selected_before.is_none() && selected_after.is_none() {
        return Err(AlignmentError::MissingSelectedBinding(selected.clone()));
    }

    let mut binding_pairs = BTreeSet::<(BindingId, BindingId, String)>::new();
    if let Some(counterpart_id) = explicit_counterpart {
        let (before_binding, after_binding) = if let Some(selected) = selected_before {
            let counterpart = after_by_id
                .get(counterpart_id)
                .copied()
                .ok_or_else(|| AlignmentError::MissingCounterpart(counterpart_id.clone()))?;
            (selected, counterpart)
        } else {
            let counterpart = before_by_id
                .get(counterpart_id)
                .copied()
                .ok_or_else(|| AlignmentError::MissingCounterpart(counterpart_id.clone()))?;
            (
                counterpart,
                selected_after.expect("one selected side was established"),
            )
        };
        validate_explicit_counterpart(before_binding, after_binding, diff)?;
        binding_pairs.insert((
            before_binding.id.clone(),
            after_binding.id.clone(),
            "explicit counterpart".into(),
        ));
    }

    let mut ambiguities = BTreeSet::new();
    align_automatic_bindings(
        before_bindings,
        after_bindings,
        diff,
        &mut binding_pairs,
        &mut ambiguities,
    );

    let ambiguous_binding_ids = ambiguous_entities(&ambiguities);
    let mut bindings = BTreeSet::new();
    let mut before_binding_tokens = BTreeMap::new();
    let mut after_binding_tokens = BTreeMap::new();
    let mut next_binding = 1_u64;
    for (before_id, after_id, evidence) in &binding_pairs {
        let logical = LogicalBindingId(next_binding);
        next_binding += 1;
        bindings.insert(BindingAlignment {
            logical,
            before: Some(before_id.clone()),
            after: Some(after_id.clone()),
            evidence: evidence.clone(),
        });
    }
    for binding in before_bindings {
        if binding_pairs.iter().any(|pair| pair.0 == binding.id)
            || ambiguous_binding_ids.contains(&AlignmentEntity::Binding(binding.id.clone()))
        {
            continue;
        }
        bindings.insert(BindingAlignment {
            logical: LogicalBindingId(next_binding),
            before: Some(binding.id.clone()),
            after: None,
            evidence: "confirmed one-sided declaration".into(),
        });
        next_binding += 1;
    }
    for binding in after_bindings {
        if binding_pairs.iter().any(|pair| pair.1 == binding.id)
            || ambiguous_binding_ids.contains(&AlignmentEntity::Binding(binding.id.clone()))
        {
            continue;
        }
        bindings.insert(BindingAlignment {
            logical: LogicalBindingId(next_binding),
            before: None,
            after: Some(binding.id.clone()),
            evidence: "confirmed one-sided declaration".into(),
        });
        next_binding += 1;
    }

    // Use the logical number, not snapshot-local IDs or declaration names, in
    // fingerprints.  This preserves a renamed binding's semantic identity.
    for alignment in &bindings {
        let token = format!("binding:{}", alignment.logical.0);
        if let Some(id) = &alignment.before {
            before_binding_tokens.insert(id.clone(), token.clone());
        }
        if let Some(id) = &alignment.after {
            after_binding_tokens.insert(id.clone(), token);
        }
    }
    let before_fingerprints = fingerprints(before, &before_binding_tokens);
    let after_fingerprints = fingerprints(after, &after_binding_tokens);

    let mut node_pairs = BTreeSet::<(NodeId, NodeId, String)>::new();
    align_synthetic_nodes(before, after, &mut node_pairs);
    for binding in &bindings {
        let (Some(before_id), Some(after_id)) = (&binding.before, &binding.after) else {
            continue;
        };
        let before_declaration = &before_by_id[before_id].declaration;
        let after_declaration = &after_by_id[after_id].declaration;
        if let (Some(old), Some(new)) = (
            declaration_write(before, before_id, before_declaration),
            declaration_write(after, after_id, after_declaration),
        ) {
            node_pairs.insert((old, new, "aligned declaration write".into()));
        }
    }
    align_source_nodes(
        before,
        after,
        &before_fingerprints,
        &after_fingerprints,
        &mut node_pairs,
        &mut ambiguities,
    );

    let ambiguous_node_ids = ambiguous_entities(&ambiguities);
    let mut nodes = BTreeSet::new();
    let mut next_node = 1_u64;
    for (before_id, after_id, evidence) in &node_pairs {
        nodes.insert(Alignment {
            logical: LogicalNodeId(next_node),
            before: Some(before_id.clone()),
            after: Some(after_id.clone()),
            evidence: evidence.clone(),
        });
        next_node += 1;
    }
    for node in before.nodes.values() {
        if node_pairs.iter().any(|pair| pair.0 == node.id)
            || ambiguous_node_ids.contains(&AlignmentEntity::Node(node.id.clone()))
        {
            continue;
        }
        nodes.insert(Alignment {
            logical: LogicalNodeId(next_node),
            before: Some(node.id.clone()),
            after: None,
            evidence: "confirmed one-sided operation".into(),
        });
        next_node += 1;
    }
    for node in after.nodes.values() {
        if node_pairs.iter().any(|pair| pair.1 == node.id)
            || ambiguous_node_ids.contains(&AlignmentEntity::Node(node.id.clone()))
        {
            continue;
        }
        nodes.insert(Alignment {
            logical: LogicalNodeId(next_node),
            before: None,
            after: Some(node.id.clone()),
            evidence: "confirmed one-sided operation".into(),
        });
        next_node += 1;
    }

    let diagnostics = ambiguities
        .iter()
        .map(|ambiguity| Diagnostic {
            code: DiagnosticCode::AmbiguousMatch,
            message: ambiguity.evidence.clone(),
            snapshot: None,
            frontier: None,
            span: None,
            affected_flows: ambiguity
                .candidates
                .iter()
                .map(|candidate| format!("{:?} -> {:?}", candidate.before, candidate.after))
                .collect(),
        })
        .collect();

    Ok(AlignmentResult {
        bindings,
        nodes,
        ambiguities,
        diagnostics,
        before_fingerprints,
        after_fingerprints,
    })
}

fn declaration_write(
    procedure: &ProcedureIr,
    binding: &BindingId,
    declaration: &SourceSpan,
) -> Option<NodeId> {
    procedure.nodes.values().find_map(|node| {
        let Operation::Write {
            target: Place::Binding(target),
            ..
        } = &node.operation
        else {
            return None;
        };
        let span = node.span.as_ref()?;
        (target == binding
            && span.path == declaration.path
            && span.byte_start == declaration.byte_start
            && span.byte_end >= declaration.byte_end)
            .then(|| node.id.clone())
    })
}

fn validate_explicit_counterpart(
    before: &AlignableBinding,
    after: &AlignableBinding,
    diff: &RepositoryDiff,
) -> Result<(), AlignmentError> {
    if !paths_correspond(&before.declaration.path, &after.declaration.path, diff) {
        return Err(AlignmentError::IncompatibleCounterpart(
            "declarations are not in the same or validated-renamed file".into(),
        ));
    }
    if before.role != after.role {
        return Err(AlignmentError::IncompatibleCounterpart(
            "a parameter cannot correspond to a local declaration".into(),
        ));
    }
    if before.scope_path != after.scope_path || before.enclosing_symbol != after.enclosing_symbol {
        return Err(AlignmentError::IncompatibleCounterpart(
            "declarations have incompatible containing procedures or lexical roles".into(),
        ));
    }
    Ok(())
}

fn align_automatic_bindings(
    before: &[AlignableBinding],
    after: &[AlignableBinding],
    diff: &RepositoryDiff,
    pairs: &mut BTreeSet<(BindingId, BindingId, String)>,
    ambiguities: &mut BTreeSet<AlignmentAmbiguity>,
) {
    let paired_before: BTreeSet<_> = pairs.iter().map(|pair| pair.0.clone()).collect();
    let paired_after: BTreeSet<_> = pairs.iter().map(|pair| pair.1.clone()).collect();
    let mut groups = BTreeMap::<String, (Vec<&AlignableBinding>, Vec<&AlignableBinding>)>::new();
    for binding in before {
        if paired_before.contains(&binding.id) {
            continue;
        }
        groups
            .entry(binding_key(binding))
            .or_default()
            .0
            .push(binding);
    }
    for binding in after {
        if paired_after.contains(&binding.id) {
            continue;
        }
        groups
            .entry(binding_key(binding))
            .or_default()
            .1
            .push(binding);
    }
    for (_, (mut left, mut right)) in groups {
        left.retain(|before| {
            right.iter().any(|after| {
                paths_correspond(&before.declaration.path, &after.declaration.path, diff)
            })
        });
        right.retain(|after| {
            left.iter().any(|before| {
                paths_correspond(&before.declaration.path, &after.declaration.path, diff)
            })
        });
        left.sort_by_key(|binding| binding.declaration.byte_start);
        right.sort_by_key(|binding| binding.declaration.byte_start);
        if left.len() == right.len() {
            for (before, after) in left.into_iter().zip(right) {
                pairs.insert((
                    before.id.clone(),
                    after.id.clone(),
                    "file mapping and lexical role".into(),
                ));
            }
        } else if !left.is_empty() && !right.is_empty() {
            ambiguities.insert(AlignmentAmbiguity {
                candidates: left
                    .iter()
                    .flat_map(|before| {
                        right.iter().map(move |after| AlignmentCandidate {
                            before: AlignmentEntity::Binding(before.id.clone()),
                            after: AlignmentEntity::Binding(after.id.clone()),
                        })
                    })
                    .collect(),
                evidence: "multiple declarations share the same lexical role".into(),
            });
        }
    }
}

fn binding_key(binding: &AlignableBinding) -> String {
    format!(
        "{:?}|{:?}|{:?}|{}",
        binding.role, binding.scope_path, binding.enclosing_symbol, binding.name
    )
}

fn paths_correspond(before: &str, after: &str, diff: &RepositoryDiff) -> bool {
    before == after
        || diff.changes.iter().any(|change| {
            change.kind == FileChangeKind::Renamed
                && change.before_path.as_deref() == Some(before)
                && change.after_path.as_deref() == Some(after)
        })
}

fn align_synthetic_nodes(
    before: &ProcedureIr,
    after: &ProcedureIr,
    pairs: &mut BTreeSet<(NodeId, NodeId, String)>,
) {
    pairs.insert((
        before.entry.clone(),
        after.entry.clone(),
        "synthetic procedure entry".into(),
    ));
    if before.exits.len() == 1 && after.exits.len() == 1 {
        pairs.insert((
            before.exits.iter().next().expect("one exit").clone(),
            after.exits.iter().next().expect("one exit").clone(),
            "synthetic procedure exit".into(),
        ));
    }
}

fn align_source_nodes(
    before: &ProcedureIr,
    after: &ProcedureIr,
    before_fingerprints: &BTreeMap<NodeId, String>,
    after_fingerprints: &BTreeMap<NodeId, String>,
    pairs: &mut BTreeSet<(NodeId, NodeId, String)>,
    ambiguities: &mut BTreeSet<AlignmentAmbiguity>,
) {
    let synthetic_before: BTreeSet<_> = pairs.iter().map(|pair| pair.0.clone()).collect();
    let synthetic_after: BTreeSet<_> = pairs.iter().map(|pair| pair.1.clone()).collect();
    let mut exact = BTreeMap::<String, (Vec<NodeId>, Vec<NodeId>)>::new();
    for node in before.nodes.values().filter(|node| node.span.is_some()) {
        if !synthetic_before.contains(&node.id) {
            exact
                .entry(before_fingerprints[&node.id].clone())
                .or_default()
                .0
                .push(node.id.clone());
        }
    }
    for node in after.nodes.values().filter(|node| node.span.is_some()) {
        if !synthetic_after.contains(&node.id) {
            exact
                .entry(after_fingerprints[&node.id].clone())
                .or_default()
                .1
                .push(node.id.clone());
        }
    }

    let mut ambiguous = BTreeSet::new();
    for (fingerprint, (mut left, mut right)) in exact {
        if left.len() != right.len() {
            let mut stable_before = BTreeSet::new();
            let mut stable_after = BTreeSet::new();
            for old in &left {
                let old_node = &before.nodes[old];
                if !matches!(old_node.operation, Operation::Literal { .. }) {
                    continue;
                }
                let Some(span) = &old_node.span else { continue };
                if left
                    .iter()
                    .filter(|id| before.nodes[*id].span.as_ref() == Some(span))
                    .count()
                    != 1
                {
                    continue;
                }
                let mut candidates = right
                    .iter()
                    .filter(|id| after.nodes[*id].span.as_ref() == Some(span));
                let Some(new) = candidates.next() else {
                    continue;
                };
                if candidates.next().is_none() {
                    pairs.insert((
                        old.clone(),
                        new.clone(),
                        "unchanged literal at identical source span".into(),
                    ));
                    stable_before.insert(old.clone());
                    stable_after.insert(new.clone());
                }
            }
            left.retain(|id| !stable_before.contains(id));
            right.retain(|id| !stable_after.contains(id));
        }
        if left.len() == right.len() {
            for (before, after) in left.into_iter().zip(right) {
                pairs.insert((before, after, "identical semantic fingerprint".into()));
            }
        } else if !left.is_empty() && !right.is_empty() {
            let candidates: BTreeSet<_> = left
                .iter()
                .flat_map(|before| {
                    right.iter().map(move |after| AlignmentCandidate {
                        before: AlignmentEntity::Node(before.clone()),
                        after: AlignmentEntity::Node(after.clone()),
                    })
                })
                .collect();
            ambiguous.extend(left.iter().cloned());
            ambiguous.extend(right.iter().cloned());
            ambiguities.insert(AlignmentAmbiguity {
                candidates,
                evidence: format!(
                    "indistinguishable repeated operations with fingerprint {fingerprint}"
                ),
            });
        }
    }

    let paired_before: BTreeSet<_> = pairs.iter().map(|pair| pair.0.clone()).collect();
    let paired_after: BTreeSet<_> = pairs.iter().map(|pair| pair.1.clone()).collect();
    let mut relaxed = BTreeMap::<&'static str, (Vec<NodeId>, Vec<NodeId>)>::new();
    for node in before.nodes.values().filter(|node| node.span.is_some()) {
        if !paired_before.contains(&node.id) && !ambiguous.contains(&node.id) {
            relaxed
                .entry(operation_kind(&node.operation))
                .or_default()
                .0
                .push(node.id.clone());
        }
    }
    for node in after.nodes.values().filter(|node| node.span.is_some()) {
        if !paired_after.contains(&node.id) && !ambiguous.contains(&node.id) {
            relaxed
                .entry(operation_kind(&node.operation))
                .or_default()
                .1
                .push(node.id.clone());
        }
    }
    for (_, (left, right)) in relaxed {
        if left.len() == 1 && right.len() == 1 {
            pairs.insert((
                left[0].clone(),
                right[0].clone(),
                "unique syntax role and local edit context".into(),
            ));
        }
    }
    align_nested_sources(before, after, pairs, ambiguities);
}

fn align_nested_sources(
    before: &ProcedureIr,
    after: &ProcedureIr,
    pairs: &mut BTreeSet<(NodeId, NodeId, String)>,
    ambiguities: &mut BTreeSet<AlignmentAmbiguity>,
) {
    loop {
        let mut additions = BTreeSet::new();
        let occupied_before: BTreeSet<_> = pairs.iter().map(|pair| pair.0.clone()).collect();
        let occupied_after: BTreeSet<_> = pairs.iter().map(|pair| pair.1.clone()).collect();
        for (old, new, _) in pairs.iter() {
            let (Some(old_node), Some(new_node)) = (before.nodes.get(old), after.nodes.get(new))
            else {
                continue;
            };
            let (old_inputs, new_inputs) = match (&old_node.operation, &new_node.operation) {
                (Operation::Write { sources: old, .. }, Operation::Write { sources: new, .. }) => (
                    old.iter().collect::<Vec<_>>(),
                    new.iter().collect::<Vec<_>>(),
                ),
                (
                    Operation::Compute { inputs: old, .. },
                    Operation::Compute { inputs: new, .. },
                ) => (
                    old.iter().map(|i| &i.place).collect::<Vec<_>>(),
                    new.iter().map(|i| &i.place).collect::<Vec<_>>(),
                ),
                _ => continue,
            };
            if old_inputs.len() != new_inputs.len() {
                continue;
            }
            for (old_place, new_place) in old_inputs.into_iter().zip(new_inputs) {
                let (Place::Temporary(old_source), Place::Temporary(new_source)) =
                    (old_place, new_place)
                else {
                    continue;
                };
                if occupied_before.contains(old_source) || occupied_after.contains(new_source) {
                    continue;
                }
                let (Some(old_source_node), Some(new_source_node)) =
                    (before.nodes.get(old_source), after.nodes.get(new_source))
                else {
                    continue;
                };
                if operation_kind(&old_source_node.operation)
                    == operation_kind(&new_source_node.operation)
                {
                    additions.insert((
                        old_source.clone(),
                        new_source.clone(),
                        "matched producer of aligned input".into(),
                    ));
                }
            }
        }
        if additions.is_empty() {
            break;
        }
        pairs.extend(additions);
    }
    let paired_before: BTreeSet<_> = pairs.iter().map(|pair| pair.0.clone()).collect();
    let paired_after: BTreeSet<_> = pairs.iter().map(|pair| pair.1.clone()).collect();
    ambiguities.retain(|ambiguity| !ambiguity.candidates.iter().all(|candidate| {
        matches!((&candidate.before, &candidate.after), (AlignmentEntity::Node(old), AlignmentEntity::Node(new)) if paired_before.contains(old) && paired_after.contains(new))
    }));
}

fn ambiguous_entities(ambiguities: &BTreeSet<AlignmentAmbiguity>) -> BTreeSet<AlignmentEntity> {
    ambiguities
        .iter()
        .flat_map(|ambiguity| {
            ambiguity
                .candidates
                .iter()
                .flat_map(|candidate| [candidate.before.clone(), candidate.after.clone()])
        })
        .collect()
}

fn fingerprints(
    procedure: &ProcedureIr,
    binding_tokens: &BTreeMap<BindingId, String>,
) -> BTreeMap<NodeId, String> {
    let mut cache = BTreeMap::new();
    for id in procedure.nodes.keys() {
        fingerprint_node(
            procedure,
            id,
            binding_tokens,
            &mut cache,
            &mut BTreeSet::new(),
        );
    }
    cache
}

fn fingerprint_node(
    procedure: &ProcedureIr,
    id: &NodeId,
    binding_tokens: &BTreeMap<BindingId, String>,
    cache: &mut BTreeMap<NodeId, String>,
    visiting: &mut BTreeSet<NodeId>,
) -> String {
    if let Some(value) = cache.get(id) {
        return value.clone();
    }
    if !visiting.insert(id.clone()) {
        return "cycle".into();
    }
    let operation = &procedure.nodes[id].operation;
    let place =
        |place: &Place, cache: &mut BTreeMap<NodeId, String>, visiting: &mut BTreeSet<NodeId>| {
            fingerprint_place(procedure, place, binding_tokens, cache, visiting)
        };
    let value = match operation {
        Operation::Entry => "entry".into(),
        Operation::Exit => "exit".into(),
        Operation::Read { source, .. } => {
            format!("read({})", place(source, cache, visiting))
        }
        Operation::Write {
            target, sources, ..
        } => format!(
            "write(target={},sources=[{}])",
            place(target, cache, visiting),
            sources
                .iter()
                .map(|source| place(source, cache, visiting))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Operation::Literal {
            literal_kind,
            raw,
            cooked,
            ..
        } => format!("literal({literal_kind:?},{raw:?},{cooked:?})"),
        Operation::Compute {
            inputs, operator, ..
        } => format!(
            "compute({operator:?},[{}])",
            inputs
                .iter()
                .map(|input| format!(
                    "{}:{}",
                    input_role(&input.role),
                    place(&input.place, cache, visiting)
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Operation::Branch { condition } => {
            format!("branch({})", place(condition, cache, visiting))
        }
        Operation::Join => "join".into(),
        Operation::Call {
            arguments, result, ..
        } => format!(
            "call([{}],result={})",
            arguments
                .iter()
                .map(|argument| place(argument, cache, visiting))
                .collect::<Vec<_>>()
                .join(","),
            result.is_some()
        ),
        Operation::ReturnSite { .. } => "return_site".into(),
        Operation::Return { value } => format!(
            "return({})",
            value
                .as_ref()
                .map(|value| place(value, cache, visiting))
                .unwrap_or_else(|| "void".into())
        ),
        Operation::UnknownEffect {
            effect_kind,
            description,
            inputs,
            result,
            affected_places,
            ..
        } => format!(
            "unknown({effect_kind:?},{description:?},inputs=[{}],result={},affected=[{}])",
            inputs
                .iter()
                .map(|input| format!(
                    "{}:{}",
                    input_role(&input.role),
                    place(&input.place, cache, visiting)
                ))
                .collect::<Vec<_>>()
                .join(","),
            result.is_some(),
            affected_places
                .iter()
                .map(|affected| place(affected, cache, visiting))
                .collect::<Vec<_>>()
                .join(",")
        ),
    };
    visiting.remove(id);
    cache.insert(id.clone(), value.clone());
    value
}

fn fingerprint_place(
    procedure: &ProcedureIr,
    place: &Place,
    binding_tokens: &BTreeMap<BindingId, String>,
    cache: &mut BTreeMap<NodeId, String>,
    visiting: &mut BTreeSet<NodeId>,
) -> String {
    match place {
        Place::Binding(binding) => binding_tokens
            .get(binding)
            .cloned()
            .unwrap_or_else(|| format!("unmatched-binding:{}", binding.local)),
        Place::Temporary(producer) => format!(
            "temporary:{}",
            fingerprint_node(procedure, producer, binding_tokens, cache, visiting)
        ),
    }
}

fn input_role(role: &ComputeInputRole) -> String {
    match role {
        ComputeInputRole::Operand { index } => format!("operand:{index}"),
        ComputeInputRole::TemplateChunk { index } => format!("chunk:{index}"),
        ComputeInputRole::TemplateInterpolation { index } => format!("interpolation:{index}"),
    }
}

fn operation_kind(operation: &Operation) -> &'static str {
    match operation {
        Operation::Entry => "entry",
        Operation::Read { .. } => "read",
        Operation::Write { .. } => "write",
        Operation::Literal { .. } => "literal",
        Operation::Compute { .. } => "compute",
        Operation::Branch { .. } => "branch",
        Operation::Join => "join",
        Operation::Call { .. } => "call",
        Operation::ReturnSite { .. } => "return_site",
        Operation::Return { .. } => "return",
        Operation::Exit => "exit",
        Operation::UnknownEffect { .. } => "unknown_effect",
    }
}

#[cfg(test)]
mod tests;
