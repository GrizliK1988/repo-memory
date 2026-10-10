//! Bounded, path-aware evidence for the branch subset introduced in K014.
//! This is a validation layer; it does not change distributive IFDS transfers.

use crate::ifds::ir::{EdgeKind, LiteralKind, Operation, PrimitiveOperator, ProcedureIr};
use crate::ifds::model::{BindingId, DefinitionId, Fact, NodeId, Place};
use crate::ifds::provenance::{ProcedureFlowFunctions, transfer_operation};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BranchState {
    pub facts: BTreeSet<Fact>,
    pub labels: BTreeSet<String>,
    pub unproven: bool,
    /// Branch outcomes are keyed by the operation that read the condition. A
    /// display label alone cannot distinguish shadowing or later reassignment.
    pub decisions: BTreeMap<NodeId, bool>,
    /// Facts captured when a path constraint was first tested. Reevaluating a
    /// loop condition must not change the input value described by that guard.
    pub guard_facts: BTreeMap<NodeId, BTreeSet<Fact>>,
    pub guard_reads: BTreeMap<NodeId, BTreeMap<NodeId, BTreeSet<DefinitionId>>>,
    pub guard_choices: BTreeMap<NodeId, BTreeMap<NodeId, bool>>,
    read_writes: BTreeMap<NodeId, BTreeSet<DefinitionId>>,
    pub carried_loops: BTreeSet<NodeId>,
    pub unknown_effects: bool,
    pub returned: bool,
    pub loop_unproven: bool,
    pub choices: BTreeMap<NodeId, bool>,
    guards: BTreeMap<BindingId, bool>,
    known: BTreeMap<Place, KnownValue>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct KnownValue {
    truthy: bool,
    nullish: bool,
}

impl KnownValue {
    fn boolean(value: bool) -> Self {
        Self {
            truthy: value,
            nullish: false,
        }
    }
}

#[derive(Clone, Default)]
pub struct BranchPaths {
    pub at: BTreeMap<NodeId, BTreeSet<BranchState>>,
    pub truncated: bool,
    pub unproven_branches: BTreeSet<NodeId>,
    pub cycles: BTreeMap<NodeId, BTreeSet<BranchState>>,
    pub cycle_backedges: BTreeSet<(NodeId, NodeId)>,
    pub cycle_witnesses: BTreeMap<(NodeId, NodeId), Vec<NodeId>>,
}

impl BranchPaths {
    pub fn build(procedure: &ProcedureIr, entry_facts: BTreeSet<Fact>, max_states: usize) -> Self {
        let flows = ProcedureFlowFunctions::new(procedure);
        let initial = BranchState {
            facts: entry_facts,
            labels: BTreeSet::new(),
            unproven: false,
            decisions: BTreeMap::new(),
            guard_facts: BTreeMap::new(),
            guard_reads: BTreeMap::new(),
            guard_choices: BTreeMap::new(),
            read_writes: BTreeMap::new(),
            carried_loops: BTreeSet::new(),
            unknown_effects: false,
            returned: false,
            loop_unproven: false,
            choices: BTreeMap::new(),
            guards: BTreeMap::new(),
            known: BTreeMap::new(),
        };
        let mut result = Self::default();
        result
            .at
            .entry(procedure.entry.clone())
            .or_default()
            .insert(initial.clone());
        let mut queue = VecDeque::from([(procedure.entry.clone(), initial)]);
        let mut state_ids = BTreeMap::new();
        state_ids.insert(queue[0].clone(), 0);
        let mut transitions = vec![BTreeSet::new()];
        let mut backedges = Vec::new();
        let mut count = 1;
        while let Some((node_id, incoming)) = queue.pop_front() {
            let from = state_ids[&(node_id.clone(), incoming.clone())];
            let node = &procedure.nodes[&node_id];
            let mut outgoing = incoming.clone();
            if let Operation::Read { source, .. } = &node.operation {
                outgoing.read_writes.insert(
                    node_id.clone(),
                    incoming
                        .facts
                        .iter()
                        .filter_map(|fact| match fact {
                            Fact::LastWrite { place, write } if place == source => {
                                Some(write.clone())
                            }
                            _ => None,
                        })
                        .collect(),
                );
            }
            outgoing.facts = if let Operation::Compute {
                operator: PrimitiveOperator::ValueJoin { branch },
                inputs,
                result,
                definition,
            } = &node.operation
            {
                let selected = incoming
                    .choices
                    .get(branch)
                    .and_then(|outcome| inputs.get(if *outcome { 0 } else { 1 }));
                if let Some(input) = selected {
                    let selected_operation = Operation::Compute {
                        definition: definition.clone(),
                        inputs: vec![input.clone()],
                        result: result.clone(),
                        operator: PrimitiveOperator::ValueJoin {
                            branch: branch.clone(),
                        },
                    };
                    incoming
                        .facts
                        .iter()
                        .flat_map(|fact| transfer_operation(&selected_operation, fact).into_facts())
                        .collect()
                } else {
                    flows.transfer_all(&node_id, &incoming.facts).into_facts()
                }
            } else {
                flows.transfer_all(&node_id, &incoming.facts).into_facts()
            };
            outgoing.update_known(&node.operation);
            outgoing.unknown_effects |= matches!(
                node.operation,
                Operation::UnknownEffect { .. } | Operation::Call { .. }
            );
            outgoing.returned |= matches!(node.operation, Operation::Return { .. });
            for edge in procedure.edges.iter().filter(|edge| edge.source == node_id) {
                if !matches!(
                    edge.kind,
                    EdgeKind::Normal | EdgeKind::Branch { .. } | EdgeKind::LoopBack
                ) {
                    continue;
                }
                let mut next = outgoing.clone();
                if edge.kind == EdgeKind::LoopBack {
                    next.carried_loops.insert(edge.target.clone());
                }
                if let EdgeKind::Branch { outcome } = edge.kind {
                    let Operation::Branch { condition } = &node.operation else {
                        continue;
                    };
                    if let Some(value) = next.known.get(condition)
                        && value.truthy != outcome
                    {
                        continue;
                    }
                    next.choices.insert(node_id.clone(), outcome);
                    let known = next.known.contains_key(condition);
                    let repeated = next.decisions.contains_key(&node_id)
                        && procedure.loops.iter().any(|loop_ir| {
                            loop_ir.members.contains(&node_id)
                                && next.carried_loops.contains(&loop_ir.header)
                        });
                    if let Some((binding, positive)) = guard_binding(procedure, condition) {
                        let required = outcome == positive;
                        if let Some(previous) = next.guards.get(&binding)
                            && *previous != required
                        {
                            continue;
                        }
                        next.guards.insert(binding, required);
                    } else if !known {
                        next.unproven = true;
                        result.unproven_branches.insert(node_id.clone());
                    }
                    if repeated {
                        if !known {
                            let previous_facts = next.guard_facts.get(&node_id);
                            let same_value =
                                guard_binding(procedure, condition).is_some_and(|(binding, _)| {
                                    if procedure.loops.iter().any(|loop_ir| {
                                        loop_ir.members.contains(&node_id)
                                            && next.carried_loops.contains(&loop_ir.header)
                                            && loop_ir.members.iter().any(|id| matches!(
                                                &procedure.nodes[id].operation,
                                                Operation::Write { target: Place::Binding(candidate), .. }
                                                    if candidate == &binding
                                            ))
                                    }) {
                                        // Revisiting one static writer does not
                                        // establish that its value is unchanged.
                                        return false;
                                    }
                                    let place = Place::Binding(binding);
                                    let writes = |facts: &BTreeSet<Fact>| {
                                        facts
                                            .iter()
                                            .filter_map(|fact| match fact {
                                                Fact::LastWrite { place: p, write }
                                                    if p == &place =>
                                                {
                                                    Some(write.clone())
                                                }
                                                _ => None,
                                            })
                                            .collect::<BTreeSet<_>>()
                                    };
                                    previous_facts.is_some_and(|facts| {
                                        !writes(facts).is_empty() && writes(facts) == writes(&incoming.facts)
                                    })
                                });
                            if !same_value {
                                // A changing repeated predicate is retained as a
                                // possible path, not a new input-region constraint.
                                next.unproven = true;
                                next.loop_unproven = true;
                                result.unproven_branches.insert(node_id.clone());
                            }
                        }
                    } else {
                        next.decisions.insert(node_id.clone(), outcome);
                        next.guard_facts
                            .insert(node_id.clone(), incoming.facts.clone());
                        next.guard_reads
                            .insert(node_id.clone(), outgoing.read_writes.clone());
                        next.guard_choices
                            .insert(node_id.clone(), next.choices.clone());
                        if let Some(label) = &edge.assumption
                            && !(known
                                && procedure.loops.iter().any(|item| item.condition == node_id))
                        {
                            next.labels.insert(normalize_label(label));
                        }
                    }
                }
                let key = (edge.target.clone(), next.clone());
                let to = if let Some(id) = state_ids.get(&key) {
                    *id
                } else {
                    let id = state_ids.len();
                    state_ids.insert(key, id);
                    transitions.push(BTreeSet::new());
                    id
                };
                transitions[from].insert(to);
                if edge.kind == EdgeKind::LoopBack {
                    backedges.push((from, to, edge.source.clone(), edge.target.clone()));
                }
                let states = result.at.entry(edge.target.clone()).or_default();
                if states.insert(next.clone()) {
                    count += 1;
                    if count > max_states {
                        result.truncated = true;
                        return result;
                    }
                    queue.push_back((edge.target.clone(), next));
                }
            }
        }
        let components = crate::ifds::loops::cyclic_components(&transitions);
        let state_nodes: BTreeMap<_, _> = state_ids
            .iter()
            .map(|((node, _), id)| (*id, node.clone()))
            .collect();
        for ((node, state), id) in &state_ids {
            if components[*id].is_some() {
                result
                    .cycles
                    .entry(node.clone())
                    .or_default()
                    .insert(state.clone());
            }
        }
        for (from, to, source, target) in backedges {
            if components[from].is_some() && components[from] == components[to] {
                let key = (source, target);
                result.cycle_backedges.insert(key.clone());
                result.cycle_witnesses.entry(key).or_insert_with(|| {
                    let mut queue = VecDeque::from([to]);
                    let mut previous = BTreeMap::from([(to, to)]);
                    while let Some(node) = queue.pop_front() {
                        if node == from {
                            break;
                        }
                        for target in &transitions[node] {
                            if components[*target] == components[from]
                                && !previous.contains_key(target)
                            {
                                previous.insert(*target, node);
                                queue.push_back(*target);
                            }
                        }
                    }
                    let mut path = vec![state_nodes[&from].clone()];
                    let mut current = from;
                    while current != to {
                        current = previous[&current];
                        path.push(state_nodes[&current].clone());
                    }
                    path.reverse();
                    path
                });
            }
        }
        result
    }

    pub fn conditions_for<F>(&self, node: &NodeId, predicate: F) -> BTreeSet<Option<String>>
    where
        F: Fn(&BTreeSet<Fact>) -> bool,
    {
        let mut cubes: BTreeSet<BTreeSet<String>> = self
            .at
            .get(node)
            .into_iter()
            .flatten()
            .filter(|state| predicate(&state.facts))
            .map(|state| state.labels.clone())
            .collect();
        loop {
            let mut next = cubes.clone();
            for left in &cubes {
                for right in &cubes {
                    let a: Vec<_> = left.difference(right).collect();
                    let b: Vec<_> = right.difference(left).collect();
                    if a.len() == 1
                        && b.len() == 1
                        && (a[0].strip_prefix('!') == Some(b[0].as_str())
                            || b[0].strip_prefix('!') == Some(a[0].as_str()))
                    {
                        next.insert(left.intersection(right).cloned().collect());
                    }
                }
            }
            let minimal: BTreeSet<_> = next
                .iter()
                .filter(|cube| {
                    !next
                        .iter()
                        .any(|other| other.len() < cube.len() && other.is_subset(cube))
                })
                .cloned()
                .collect();
            if minimal == cubes {
                break;
            }
            cubes = minimal;
        }
        cubes
            .into_iter()
            .map(|labels| {
                (!labels.is_empty()).then(|| labels.into_iter().collect::<Vec<_>>().join(" && "))
            })
            .collect()
    }

    pub fn condition_unproven(&self, node: &NodeId, condition: &Option<String>) -> bool {
        self.at.get(node).is_some_and(|states| {
            states.iter().any(|state| {
                state.unproven
                    && condition.as_ref().is_none_or(|label| {
                        label.split(" && ").all(|term| state.labels.contains(term))
                    })
            })
        })
    }
}

impl BranchState {
    fn update_known(&mut self, operation: &Operation) {
        match operation {
            Operation::Literal {
                literal_kind,
                raw,
                cooked,
                result,
                ..
            } => {
                self.known.remove(result);
                let value = match literal_kind {
                    LiteralKind::Boolean => Some(KnownValue::boolean(raw == "true")),
                    LiteralKind::Null | LiteralKind::Undefined => Some(KnownValue {
                        truthy: false,
                        nullish: true,
                    }),
                    LiteralKind::Number => known_number(raw).map(KnownValue::boolean),
                    LiteralKind::BigInt => {
                        known_number(raw.trim_end_matches('n')).map(KnownValue::boolean)
                    }
                    LiteralKind::String => {
                        known_string(raw, cooked.as_deref()).map(KnownValue::boolean)
                    }
                    LiteralKind::TemplateChunk => None,
                };
                if let Some(value) = value {
                    self.known.insert(result.clone(), value);
                }
            }
            Operation::Read { source, result } => {
                if let Some(value) = self.known.get(source).copied() {
                    self.known.insert(result.clone(), value);
                } else {
                    self.known.remove(result);
                }
            }
            Operation::Compute {
                operator: PrimitiveOperator::LogicalNot,
                inputs,
                result,
                ..
            } => {
                if let Some(value) = inputs
                    .first()
                    .and_then(|input| self.known.get(&input.place))
                    .copied()
                {
                    self.known
                        .insert(result.clone(), KnownValue::boolean(!value.truthy));
                } else {
                    self.known.remove(result);
                }
            }
            Operation::Compute {
                operator: PrimitiveOperator::IsNullish,
                inputs,
                result,
                ..
            } => {
                if let Some(value) = inputs
                    .first()
                    .and_then(|input| self.known.get(&input.place))
                    .copied()
                {
                    self.known
                        .insert(result.clone(), KnownValue::boolean(value.nullish));
                } else {
                    self.known.remove(result);
                }
            }
            Operation::Compute {
                operator: PrimitiveOperator::ValueJoin { branch },
                inputs,
                result,
                ..
            } => {
                let index = self
                    .choices
                    .get(branch)
                    .map(|outcome| if *outcome { 0 } else { 1 });
                if let Some(value) = index
                    .and_then(|index| inputs.get(index))
                    .and_then(|input| self.known.get(&input.place))
                    .copied()
                {
                    self.known.insert(result.clone(), value);
                } else {
                    self.known.remove(result);
                }
            }
            Operation::Write {
                target, sources, ..
            } => {
                let value = (sources.len() == 1)
                    .then(|| sources.iter().next().unwrap())
                    .and_then(|source| self.known.get(source))
                    .copied();
                if let Some(value) = value {
                    self.known.insert(target.clone(), value);
                } else {
                    self.known.remove(target);
                }
                if let Place::Binding(binding) = target {
                    self.guards.remove(binding);
                }
            }
            Operation::Compute { result, .. } => {
                self.known.remove(result);
            }
            Operation::UnknownEffect {
                affected_places,
                result,
                ..
            } => {
                for place in affected_places.iter().chain(result.iter()) {
                    self.known.remove(place);
                    if let Place::Binding(binding) = place {
                        self.guards.remove(binding);
                    }
                }
            }
            _ => {}
        }
    }
}

fn known_number(raw: &str) -> Option<bool> {
    let compact = raw.replace('_', "");
    if let Some(hex) = compact
        .strip_prefix("0x")
        .or_else(|| compact.strip_prefix("0X"))
    {
        return u128::from_str_radix(hex, 16).ok().map(|value| value != 0);
    }
    if let Some(binary) = compact
        .strip_prefix("0b")
        .or_else(|| compact.strip_prefix("0B"))
    {
        return u128::from_str_radix(binary, 2).ok().map(|value| value != 0);
    }
    if let Some(octal) = compact
        .strip_prefix("0o")
        .or_else(|| compact.strip_prefix("0O"))
    {
        return u128::from_str_radix(octal, 8).ok().map(|value| value != 0);
    }
    compact
        .parse::<f64>()
        .ok()
        .map(|value| value != 0.0 && !value.is_nan())
}

fn known_string(raw: &str, cooked: Option<&str>) -> Option<bool> {
    if let Some(cooked) = cooked {
        return Some(!cooked.is_empty());
    }
    if raw == "''" {
        return Some(false);
    }
    if raw.starts_with('\'') && raw.ends_with('\'') && !raw.contains('\\') {
        return Some(true);
    }
    None
}

fn guard_binding(procedure: &ProcedureIr, place: &Place) -> Option<(BindingId, bool)> {
    let Place::Temporary(id) = place else {
        return None;
    };
    match &procedure.nodes.get(id)?.operation {
        Operation::Read {
            source: Place::Binding(binding),
            ..
        } => Some((binding.clone(), true)),
        Operation::Compute {
            operator: PrimitiveOperator::LogicalNot,
            inputs,
            ..
        } => {
            let (binding, positive) = guard_binding(procedure, &inputs.first()?.place)?;
            Some((binding, !positive))
        }
        _ => None,
    }
}

fn normalize_label(label: &str) -> String {
    let compact: String = label.chars().filter(|ch| !ch.is_whitespace()).collect();
    let mut negative = false;
    let mut core = compact.as_str();
    loop {
        if let Some(rest) = core.strip_prefix('!') {
            negative = !negative;
            core = rest;
        } else if core.starts_with('(') && core.ends_with(')') && encloses_whole(core) {
            core = &core[1..core.len() - 1];
        } else {
            break;
        }
    }
    if negative {
        format!("!{core}")
    } else {
        core.to_owned()
    }
}

fn encloses_whole(text: &str) -> bool {
    let mut depth = 0;
    for (index, ch) in text.char_indices() {
        if ch == '(' {
            depth += 1;
        }
        if ch == ')' {
            depth -= 1;
        }
        if depth == 0 && index + ch.len_utf8() < text.len() {
            return false;
        }
    }
    depth == 0
}
