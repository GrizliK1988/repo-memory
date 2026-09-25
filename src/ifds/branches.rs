//! Bounded, path-aware evidence for the branch subset introduced in K014.
//! This is a validation layer; it does not change distributive IFDS transfers.

use crate::ifds::ir::{EdgeKind, LiteralKind, Operation, PrimitiveOperator, ProcedureIr};
use crate::ifds::model::{BindingId, Fact, NodeId, Place};
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

#[derive(Default)]
pub struct BranchPaths {
    pub at: BTreeMap<NodeId, BTreeSet<BranchState>>,
    pub truncated: bool,
    pub unproven_branches: BTreeSet<NodeId>,
}

impl BranchPaths {
    pub fn build(procedure: &ProcedureIr, entry_facts: BTreeSet<Fact>, max_states: usize) -> Self {
        let flows = ProcedureFlowFunctions::new(procedure);
        let initial = BranchState {
            facts: entry_facts,
            labels: BTreeSet::new(),
            unproven: false,
            decisions: BTreeMap::new(),
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
        let mut count = 1;
        while let Some((node_id, incoming)) = queue.pop_front() {
            let node = &procedure.nodes[&node_id];
            let mut outgoing = incoming.clone();
            outgoing.facts = if let Operation::Compute {
                operator: PrimitiveOperator::ValueJoin { branch },
                inputs,
                result,
                definition,
            } = &node.operation
            {
                let selected = incoming
                    .decisions
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
            for edge in procedure.edges.iter().filter(|edge| edge.source == node_id) {
                if !matches!(edge.kind, EdgeKind::Normal | EdgeKind::Branch { .. }) {
                    continue;
                }
                let mut next = outgoing.clone();
                if let EdgeKind::Branch { outcome } = edge.kind {
                    let Operation::Branch { condition } = &node.operation else {
                        continue;
                    };
                    if let Some(value) = next.known.get(condition)
                        && value.truthy != outcome
                    {
                        continue;
                    }
                    next.decisions.insert(node_id.clone(), outcome);
                    if let Some((binding, positive)) = guard_binding(procedure, condition) {
                        let required = outcome == positive;
                        if let Some(previous) = next.guards.get(&binding)
                            && *previous != required
                        {
                            continue;
                        }
                        next.guards.insert(binding, required);
                    } else if !next.known.contains_key(condition) {
                        next.unproven = true;
                        result.unproven_branches.insert(node_id.clone());
                    }
                    if let Some(label) = &edge.assumption {
                        next.labels.insert(normalize_label(label));
                    }
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
        result
    }

    pub fn conditions_for<F>(&self, node: &NodeId, predicate: F) -> BTreeSet<Option<String>>
    where
        F: Fn(&BTreeSet<Fact>) -> bool,
    {
        self.at
            .get(node)
            .into_iter()
            .flatten()
            .filter(|state| predicate(&state.facts))
            .map(|state| {
                (!state.labels.is_empty()).then(|| {
                    state
                        .labels
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" && ")
                })
            })
            .collect()
    }

    pub fn condition_unproven(&self, node: &NodeId, condition: &Option<String>) -> bool {
        self.at.get(node).is_some_and(|states| {
            states.iter().any(|state| {
                let label = (!state.labels.is_empty()).then(|| {
                    state
                        .labels
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" && ")
                });
                &label == condition && state.unproven
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
                }
            }
            Operation::Compute {
                operator: PrimitiveOperator::ValueJoin { branch },
                inputs,
                result,
                ..
            } => {
                let index = self
                    .decisions
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
