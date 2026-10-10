//! Compare recorded loop source operations without claiming runtime equality.

use super::*;

type Pairs = Vec<(usize, usize)>;

/// Detailed output keeps each snapshot; short output describes paired edits once.
pub(crate) fn render_loop_comparison(
    before: &[LoopSummary],
    after: &[LoopSummary],
    verbose: bool,
) -> Vec<String> {
    if verbose {
        let mut lines = render_loop_summaries(before, "before");
        lines.extend(render_loop_summaries(after, "after"));
        return lines;
    }
    let mut lines = Vec::new();
    render_scope(before, after, None, None, &mut lines);
    lines
}

fn parent(loops: &[LoopSummary], index: usize) -> Option<usize> {
    let span = &loops[index].span;
    loops
        .iter()
        .enumerate()
        .filter(|(candidate, item)| {
            *candidate != index
                && item.span.path == span.path
                && item.span.byte_start <= span.byte_start
                && item.span.byte_end >= span.byte_end
                && item.span != *span
        })
        .min_by_key(|(_, item)| item.span.byte_end - item.span.byte_start)
        .map(|(index, _)| index)
}

fn direct_operations(loops: &[LoopSummary], index: usize) -> Vec<&LoopOperation> {
    let summary = &loops[index];
    summary
        .operations
        .iter()
        .filter(|operation| {
            operation.node != summary.condition
                && matches!(
                    operation.operation.as_str(),
                    "write" | "condition" | "return" | "break" | "continue" | "unknown"
                )
                && !loops.iter().enumerate().any(|(child, item)| {
                    parent(loops, child) == Some(index)
                        && operation.span.byte_start >= item.span.byte_start
                        && operation.span.byte_end <= item.span.byte_end
                })
        })
        .collect()
}

// Never zip by occurrence or source line. Repeated indistinguishable candidates
// remain unpaired. Callers match siblings only, preserving nested-loop context.
fn pair_unique<K: Ord>(
    before: &mut BTreeSet<usize>,
    after: &mut BTreeSet<usize>,
    before_key: impl Fn(usize) -> Option<K>,
    after_key: impl Fn(usize) -> Option<K>,
    pairs: &mut Pairs,
) {
    let mut candidates = BTreeMap::<K, (Vec<usize>, Vec<usize>)>::new();
    for &index in before.iter() {
        if let Some(key) = before_key(index) {
            candidates.entry(key).or_default().0.push(index);
        }
    }
    for &index in after.iter() {
        if let Some(key) = after_key(index) {
            candidates.entry(key).or_default().1.push(index);
        }
    }
    for (left, right) in candidates.into_values() {
        if left.len() == 1 && right.len() == 1 {
            before.remove(&left[0]);
            after.remove(&right[0]);
            pairs.push((left[0], right[0]));
        }
    }
}

fn body_key(loops: &[LoopSummary], index: usize) -> Option<Vec<(String, String)>> {
    let operations: Vec<_> = direct_operations(loops, index)
        .iter()
        .map(|operation| (operation.operation.clone(), operation.text.clone()))
        .collect();
    (!operations.is_empty()).then_some(operations)
}

fn render_scope(
    before: &[LoopSummary],
    after: &[LoopSummary],
    before_parent: Option<usize>,
    after_parent: Option<usize>,
    lines: &mut Vec<String>,
) {
    let mut left: BTreeSet<_> = (0..before.len())
        .filter(|&index| parent(before, index) == before_parent)
        .collect();
    let mut right: BTreeSet<_> = (0..after.len())
        .filter(|&index| parent(after, index) == after_parent)
        .collect();
    let mut pairs = Vec::new();
    // First retain unique exact source matches, then unchanged guards, then
    // unchanged bodies (which can establish a changed guard or loop kind).
    pair_unique(
        &mut left,
        &mut right,
        |index| Some((header(&before[index]), body_key(before, index))),
        |index| Some((header(&after[index]), body_key(after, index))),
        &mut pairs,
    );
    pair_unique(
        &mut left,
        &mut right,
        |index| Some((kind(&before[index]), &before[index].condition_text)),
        |index| Some((kind(&after[index]), &after[index].condition_text)),
        &mut pairs,
    );
    pair_unique(
        &mut left,
        &mut right,
        |index| body_key(before, index),
        |index| body_key(after, index),
        &mut pairs,
    );
    pairs.sort_unstable();
    for (a, b) in pairs {
        render_pair(before, after, a, b, lines);
        render_scope(before, after, Some(a), Some(b), lines);
    }
    if !left.is_empty() && !right.is_empty() {
        lines.push("Loop correspondence unresolved; unmatched snapshot details follow".into());
    }
    for (loops, indices, side) in [(before, left, "before only"), (after, right, "after only")] {
        for index in indices {
            // Reachability or unsupported syntax can hide a counterpart; absence
            // of a recorded summary alone does not prove source deletion/addition.
            lines.extend(render_loop_summaries(
                std::slice::from_ref(&loops[index]),
                side,
            ));
            let descendants: Vec<_> = loops
                .iter()
                .filter(|item| {
                    item.span.byte_start > loops[index].span.byte_start
                        && item.span.byte_end <= loops[index].span.byte_end
                })
                .cloned()
                .collect();
            lines.extend(render_loop_summaries(&descendants, side));
        }
    }
}

fn kind(summary: &LoopSummary) -> &'static str {
    match summary.kind {
        LoopKind::While => "while",
        LoopKind::DoWhile => "do/while",
        LoopKind::For => "for",
    }
}

fn header(summary: &LoopSummary) -> String {
    format!("{} {}", kind(summary), summary.condition_text)
}

fn locations(before: &SourceSpan, after: &SourceSpan) -> String {
    format!(
        "before {}:{}; after {}:{}",
        before.path, before.start_line, after.path, after.start_line
    )
}

fn inputs(summary: &LoopSummary) -> String {
    summary
        .inputs
        .iter()
        .map(|input| {
            if input.carried {
                format!("{} (loop carried)", input.name)
            } else {
                input.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn execution(summary: &LoopSummary) -> &'static str {
    if summary.feasibility != Coverage::Complete && !summary.may_repeat {
        "possible execution; execution count is not established"
    } else if !summary.may_enter {
        "proven unreachable"
    } else if summary.may_repeat {
        "may repeat; execution count is not established"
    } else if summary.may_skip {
        "may be skipped; executes at most once"
    } else {
        "executes once on represented paths"
    }
}

fn operation_key(operation: &LoopOperation) -> (String, String) {
    let target = if operation.operation == "write" {
        // Supported writes are plain binding assignments, including initializers.
        operation
            .text
            .split_once('=')
            .map_or("", |(target, _)| target.trim())
    } else {
        ""
    };
    (operation.operation.clone(), target.to_owned())
}

fn operation_label(operation: &LoopOperation) -> &'static str {
    match operation.operation.as_str() {
        "write" => "Write",
        "condition" => "Condition",
        "return" => "Return",
        "break" => "Break",
        "continue" => "Continue",
        _ => "Unsupported operation",
    }
}

fn render_pair(
    before: &[LoopSummary],
    after: &[LoopSummary],
    a: usize,
    b: usize,
    lines: &mut Vec<String>,
) {
    let old = &before[a];
    let new = &after[b];
    let mut edits = Vec::new();
    if old.kind != new.kind {
        edits.push(format!("  Kind changed: {} -> {}", kind(old), kind(new)));
    }
    if old.condition_text != new.condition_text {
        edits.push(format!(
            "  Condition changed: {} -> {}",
            old.condition_text, new.condition_text
        ));
    }
    for (label, left, right) in [
        ("Initializer", &old.initializer, &new.initializer),
        ("Update", &old.update, &new.update),
    ] {
        if left != right {
            edits.push(format!(
                "  {label} changed: {} -> {}",
                left.as_deref().unwrap_or("none"),
                right.as_deref().unwrap_or("none")
            ));
        }
    }
    let old_ops = direct_operations(before, a);
    let new_ops = direct_operations(after, b);
    let mut left = (0..old_ops.len()).collect();
    let mut right = (0..new_ops.len()).collect();
    let mut pairs = Vec::new();
    pair_unique(
        &mut left,
        &mut right,
        |index| Some((&old_ops[index].operation, &old_ops[index].text)),
        |index| Some((&new_ops[index].operation, &new_ops[index].text)),
        &mut pairs,
    );
    pair_unique(
        &mut left,
        &mut right,
        |index| Some(operation_key(old_ops[index])),
        |index| Some(operation_key(new_ops[index])),
        &mut pairs,
    );
    pairs.sort_unstable();
    let mut repeated_edit = false;
    for &(i, j) in &pairs {
        let old_op = old_ops[i];
        let new_op = new_ops[j];
        if old_op.text != new_op.text {
            edits.push(format!(
                "  {} changed: {} -> {} ({})",
                operation_label(old_op),
                old_op.text,
                new_op.text,
                locations(&old_op.span, &new_op.span)
            ));
            repeated_edit |= old_op.may_repeat || new_op.may_repeat;
        }
    }
    if pairs.windows(2).any(|pair| pair[0].1 > pair[1].1) {
        edits.push("  Body operation order changed".into());
        repeated_edit |= old.may_repeat || new.may_repeat;
    }
    for (operations, indices, side) in [(&old_ops, left, "before"), (&new_ops, right, "after")] {
        for index in indices {
            let operation = operations[index];
            edits.push(format!(
                "  {} only {side}: {} ({}:{})",
                operation_label(operation),
                operation.text,
                operation.span.path,
                operation.span.start_line
            ));
            repeated_edit |= operation.may_repeat;
        }
    }
    lines.push(format!(
        "Loop{}: {} ({})",
        if edits.is_empty() {
            " context"
        } else {
            " change"
        },
        header(new),
        locations(&old.span, &new.span)
    ));
    lines.extend(edits);
    let old_inputs = inputs(old);
    let new_inputs = inputs(new);
    if old_inputs == new_inputs && !new_inputs.is_empty() {
        lines.push(format!("  Inputs: {new_inputs}"));
    } else if old_inputs != new_inputs {
        lines.push(format!("  Inputs: {old_inputs} -> {new_inputs}"));
    }
    if repeated_edit {
        lines.push("  Repetition: changed operations may run on multiple iterations; iteration count and accumulated values are unresolved".into());
    }
    if execution(old) != execution(new) {
        lines.push(format!(
            "  Body execution changed: {} -> {}",
            execution(old),
            execution(new)
        ));
    } else if !repeated_edit {
        lines.push(format!("  Body: {}", execution(new)));
    }
    if old.continuing_cycle || new.continuing_cycle {
        lines.push("  Continuing cycle: runtime termination is not established".into());
    }
    if old.feasibility != Coverage::Complete || new.feasibility != Coverage::Complete {
        lines.push("  Loop path feasibility: unresolved".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ifds::adapters::typescript::{index_bindings, lower_containing_procedure};
    use crate::ifds::model::{SnapshotId, SnapshotSide};
    use crate::ifds::provenance::seed_entry_facts;

    fn summaries(source: &str) -> Vec<LoopSummary> {
        let snapshot = SnapshotId {
            side: SnapshotSide::Before,
            revision: "test".into(),
            content_id: "tree".into(),
        };
        let index = index_bindings(snapshot, "loop.ts", source).unwrap();
        let binding = &index
            .bindings
            .iter()
            .find(|item| item.name == "x")
            .unwrap()
            .id;
        let ir = lower_containing_procedure(source, &index, binding)
            .unwrap()
            .procedure;
        let paths = BranchPaths::build(&ir, seed_entry_facts(&ir), 4096);
        assert!(!paths.truncated);
        summarize_loops(&ir, &paths, &index, source)
    }

    fn compare(before: &str, after: &str) -> String {
        render_loop_comparison(&summaries(before), &summaries(after), false).join("\n")
    }

    #[test]
    fn loop_text_condition_edit_and_reordered_writes() {
        let before = "function f(n: number, step: number) { let x = 0; while (n > 0) { x = x + step; n = n - 1; } return x; }";
        let after = before.replace("n > 0", "n > 1");
        let report = compare(before, &after);
        assert!(
            report.contains("Condition changed: (n > 0) -> (n > 1)"),
            "{report}"
        );
        assert!(!report.contains("Write changed:"), "{report}");
        assert_eq!(report.matches("Loop change:").count(), 1, "{report}");
        let after = before.replace("x = x + step; n = n - 1;", "n = n - 1; x = x + step;");
        let report = compare(before, &after);
        assert!(report.contains("Body operation order changed"), "{report}");
    }

    #[test]
    fn loop_text_nested_edits_appear_once() {
        let before = "function f(flag: boolean, inner: boolean) { let x = 0; while (flag) { while (inner) { x = 1; inner = false; } flag = false; } return x; }";
        let after = before.replace("x = 1", "x = 2");
        let report = compare(before, &after);
        assert_eq!(
            report.matches("Write changed: x = 1 -> x = 2").count(),
            1,
            "{report}"
        );
        assert!(!report.contains("Write only"), "{report}");
    }

    #[test]
    fn loop_text_duplicate_guards_do_not_invent_correspondence() {
        let source = "function f(flag: boolean) { let x = 0; while (flag) { x = 1; break; } while (flag) { x = 1; break; } return x; }";
        let report = compare(source, source);
        assert!(
            report.contains("Loop correspondence unresolved"),
            "{report}"
        );
        assert!(!report.contains("Loop change:"), "{report}");
        assert!(!report.contains("Write changed:"), "{report}");
    }

    #[test]
    fn loop_text_reachability_changes_preserve_count_uncertainty() {
        let before = "function f(flag: boolean) { let x = 0; while (flag) { x = 1; flag = false; } return x; }";
        let after = before.replace("flag = false", "flag = true");
        let report = compare(before, &after);
        assert!(
            report.contains("Write changed: flag = false -> flag = true"),
            "{report}"
        );
        assert!(
            report.contains(
                "Body execution changed: may be skipped; executes at most once -> may repeat"
            ),
            "{report}"
        );
        assert!(
            report.contains("runtime termination is not established"),
            "{report}"
        );
    }
}
