//! Finite loop structure and source-mapped possible effects. Counts and runtime
//! termination are deliberately separate from graph/fact convergence.

use super::adapters::typescript::TypeScriptBindingIndex;
use super::branches::BranchPaths;
use super::ir::{LoopKind, Operation, ProcedureIr};
use super::model::{BindingId, Coverage, Fact, NodeId, Place, Source, SourceSpan};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

mod text;
pub(crate) use text::render_loop_comparison;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopInput {
    pub binding: BindingId,
    pub name: String,
    pub origins: BTreeSet<Source>,
    pub carried: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopOperation {
    pub node: NodeId,
    pub span: SourceSpan,
    pub operation: String,
    pub text: String,
    pub may_repeat: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopSummary {
    pub kind: LoopKind,
    pub span: SourceSpan,
    pub condition: NodeId,
    pub condition_text: String,
    pub initializer: Option<String>,
    pub update: Option<String>,
    pub inputs: Vec<LoopInput>,
    pub operations: Vec<LoopOperation>,
    pub exits: Vec<LoopOperation>,
    pub may_enter: bool,
    pub may_skip: bool,
    pub may_repeat: bool,
    pub continuing_cycle: bool,
    pub feasibility: Coverage,
}

fn snippet(source: &str, span: &SourceSpan) -> String {
    source
        .get(span.byte_start as usize..span.byte_end as usize)
        .unwrap_or("")
        .trim()
        .to_owned()
}

pub fn summarize_loops(
    procedure: &ProcedureIr,
    paths: &BranchPaths,
    index: &TypeScriptBindingIndex,
    source: &str,
) -> Vec<LoopSummary> {
    let mut summaries = Vec::new();
    for loop_ir in &procedure.loops {
        if !paths.truncated
            && !paths.at.contains_key(&loop_ir.header)
            && !paths.at.contains_key(&loop_ir.body_entry)
        {
            continue;
        }
        let body_states = paths.at.get(&loop_ir.body_entry);
        let may_enter = paths.truncated || body_states.is_some_and(|states| !states.is_empty());
        let may_repeat = paths.truncated
            || body_states.is_some_and(|states| {
                states
                    .iter()
                    .any(|state| state.carried_loops.contains(&loop_ir.header))
            });
        let may_skip = loop_ir.kind != LoopKind::DoWhile
            && (paths.truncated
                || paths.at.get(&loop_ir.exit).is_some_and(|states| {
                    states.iter().any(|state| {
                        state.decisions.get(&loop_ir.condition) == Some(&false)
                            && !state.carried_loops.contains(&loop_ir.header)
                    })
                }));
        let mut inputs = BTreeMap::<BindingId, LoopInput>::new();
        let mut operations = Vec::new();
        let mut exits = Vec::new();
        let empty_states = BTreeSet::new();
        for node in loop_ir
            .members
            .iter()
            .filter_map(|id| procedure.nodes.get(id))
        {
            let states = if let Some(states) = paths.at.get(&node.id) {
                states
            } else if paths.truncated {
                &empty_states
            } else {
                continue;
            };
            if let Operation::Read {
                source: Place::Binding(binding),
                ..
            } = &node.operation
            {
                let input = inputs.entry(binding.clone()).or_insert_with(|| LoopInput {
                    binding: binding.clone(),
                    name: index
                        .bindings
                        .iter()
                        .find(|item| &item.id == binding)
                        .map_or_else(
                            || format!("binding{}", binding.local),
                            |item| item.name.clone(),
                        ),
                    origins: BTreeSet::new(),
                    carried: false,
                });
                for state in states {
                    for fact in &state.facts {
                        if let Fact::Origin {
                            place: Place::Binding(candidate),
                            source,
                        } = fact
                            && candidate == binding
                        {
                            input.origins.insert(source.clone());
                        }
                    }
                }
                input.carried |= states.iter().any(|state| state.carried_loops.contains(&loop_ir.header))
                    && loop_ir.members.iter().any(|id| matches!(&procedure.nodes[id].operation,
                        Operation::Write { target: Place::Binding(candidate), .. } if candidate == binding));
            }
            let Some(span) = &node.span else { continue };
            let text = snippet(source, span);
            let operation = match &node.operation {
                Operation::Read { .. } => "read",
                Operation::Write { .. } => "write",
                Operation::Compute { .. } => "compute",
                Operation::Branch { .. } => "condition",
                Operation::Return { .. } => "return",
                Operation::UnknownEffect { .. } => "unknown",
                Operation::Join if text.starts_with("break") => "break",
                Operation::Join if text.starts_with("continue") => "continue",
                _ => continue,
            };
            let item = LoopOperation {
                node: node.id.clone(),
                span: span.clone(),
                operation: operation.into(),
                text,
                may_repeat: paths.truncated
                    || states
                        .iter()
                        .any(|state| state.carried_loops.contains(&loop_ir.header)),
            };
            if operation == "return"
                || (operation == "break"
                    && procedure
                        .edges
                        .iter()
                        .any(|edge| edge.source == node.id && edge.target == loop_ir.exit))
            {
                exits.push(item.clone());
            }
            if operation == "condition"
                && node.id == loop_ir.condition
                && (paths.truncated
                    || paths.at.get(&loop_ir.exit).is_some_and(|states| {
                        states
                            .iter()
                            .any(|state| state.choices.get(&loop_ir.condition) == Some(&false))
                    }))
            {
                let mut false_exit = item.clone();
                false_exit.operation = "condition_false".into();
                exits.push(false_exit);
            }
            operations.push(item);
        }
        let continuing_cycle = paths
            .cycle_backedges
            .iter()
            .any(|(_, target)| target == &loop_ir.header);
        let feasibility = if paths.truncated
            || loop_ir
                .members
                .iter()
                .any(|id| paths.unproven_branches.contains(id))
        {
            Coverage::Partial
        } else {
            Coverage::Complete
        };
        summaries.push(LoopSummary {
            kind: loop_ir.kind,
            span: loop_ir.span.clone(),
            condition: loop_ir.condition.clone(),
            condition_text: procedure.nodes[&loop_ir.condition]
                .span
                .as_ref()
                .map_or_else(
                    || "true".into(),
                    |span| {
                        if span == &loop_ir.span {
                            "true".into()
                        } else {
                            snippet(source, span)
                        }
                    },
                ),
            initializer: loop_ir
                .initializer
                .as_ref()
                .map(|span| snippet(source, span)),
            update: loop_ir.update.as_ref().map(|span| snippet(source, span)),
            inputs: inputs.into_values().collect(),
            operations,
            exits,
            may_enter,
            may_skip,
            may_repeat,
            continuing_cycle,
            feasibility,
        });
    }
    summaries.sort_by_key(|item| item.span.byte_start);
    summaries
}

pub fn render_loop_summaries(summaries: &[LoopSummary], side: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for summary in summaries {
        let kind = match summary.kind {
            LoopKind::While => "while",
            LoopKind::DoWhile => "do/while",
            LoopKind::For => "for",
        };
        lines.push(format!(
            "Loop ({side}): {kind} {} ({}:{})",
            summary.condition_text, summary.span.path, summary.span.start_line
        ));
        if let Some(initializer) = &summary.initializer {
            lines.push(format!("  Initializer: {initializer}"));
        }
        if let Some(update) = &summary.update {
            lines.push(format!("  Update: {update}"));
        }
        if !summary.inputs.is_empty() {
            lines.push(format!(
                "  Inputs: {}",
                summary
                    .inputs
                    .iter()
                    .map(|input| if input.carried {
                        format!("{} (loop carried)", input.name)
                    } else {
                        input.name.clone()
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        lines.push(
            if summary.feasibility != Coverage::Complete && !summary.may_repeat {
                "  Body: possible execution; execution count is not established".into()
            } else if !summary.may_enter {
                "  Body: proven unreachable".into()
            } else if summary.may_repeat {
                format!(
                    "  Body: may repeat depending on {}; execution count is not established",
                    summary.condition_text
                )
            } else if summary.may_skip {
                "  Body: may be skipped; executes at most once".into()
            } else {
                "  Body: executes once on represented paths".into()
            },
        );
        for operation in &summary.operations {
            if operation.operation == "write" {
                lines.push(format!(
                    "  {}write: {} ({}:{})",
                    if operation.may_repeat {
                        "Possible repeated "
                    } else {
                        "Possible "
                    },
                    operation.text,
                    operation.span.path,
                    operation.span.start_line
                ));
            }
        }
        if summary.continuing_cycle {
            lines.push("  Continuing cycle: runtime termination is not established".into());
        }
        if summary.feasibility != Coverage::Complete {
            lines.push("  Loop path feasibility: unresolved".into());
        }
    }
    lines
}

/// Iterative Kosaraju over finite abstract states. Only nontrivial SCCs (or
/// self-edges) receive a component ID; a traversed backedge can be acyclic.
pub(crate) fn cyclic_components(edges: &[BTreeSet<usize>]) -> Vec<Option<usize>> {
    let mut reverse = vec![Vec::new(); edges.len()];
    for (from, targets) in edges.iter().enumerate() {
        for to in targets {
            reverse[*to].push(from);
        }
    }
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    for start in 0..edges.len() {
        let mut stack = vec![(start, false)];
        while let Some((node, finish)) = stack.pop() {
            if finish {
                order.push(node);
                continue;
            }
            if !seen.insert(node) {
                continue;
            }
            stack.push((node, true));
            stack.extend(edges[node].iter().rev().map(|target| (*target, false)));
        }
    }
    seen.clear();
    let mut result = vec![None; edges.len()];
    for start in order.into_iter().rev() {
        if seen.contains(&start) {
            continue;
        }
        let mut members = Vec::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            if !seen.insert(node) {
                continue;
            }
            members.push(node);
            stack.extend(&reverse[node]);
        }
        if members.len() > 1 || edges[start].contains(&start) {
            for node in members {
                result[node] = Some(start);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests;
