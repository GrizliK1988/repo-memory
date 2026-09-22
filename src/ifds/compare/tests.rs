use super::*;
use crate::ifds::adapters::typescript::{
    BindingKind, TypeScriptBindingIndex, TypeScriptLoweringResult, alignable_bindings,
    index_bindings, lower_containing_procedure,
};
use crate::ifds::ir::Operation;
use crate::ifds::model::{FileChange, RepositoryDiff, SnapshotId, SnapshotSide};

fn snapshot(side: SnapshotSide) -> SnapshotId {
    SnapshotId {
        side,
        revision: match side {
            SnapshotSide::Before => "before",
            SnapshotSide::After => "after",
        }
        .into(),
        content_id: match side {
            SnapshotSide::Before => "before-tree",
            SnapshotSide::After => "after-tree",
        }
        .into(),
    }
}

fn lower(
    side: SnapshotSide,
    path: &str,
    source: &str,
    name: &str,
    kind: Option<BindingKind>,
) -> (TypeScriptBindingIndex, TypeScriptLoweringResult) {
    let index = index_bindings(snapshot(side), path, source).unwrap();
    let binding = index
        .bindings
        .iter()
        .find(|binding| binding.name == name && kind.is_none_or(|kind| binding.kind == kind))
        .unwrap()
        .id
        .clone();
    let lowered = lower_containing_procedure(source, &index, &binding).unwrap();
    (index, lowered)
}

fn modified_diff(path: &str) -> RepositoryDiff {
    RepositoryDiff {
        before_content_id: "before-tree".into(),
        after_content_id: "after-tree".into(),
        changes: BTreeSet::from([FileChange {
            kind: FileChangeKind::Modified,
            before_path: Some(path.into()),
            after_path: Some(path.into()),
            before_content_id: Some("before-file".into()),
            after_content_id: Some("after-file".into()),
        }]),
    }
}

fn renamed_diff(before: &str, after: &str) -> RepositoryDiff {
    RepositoryDiff {
        before_content_id: "before-tree".into(),
        after_content_id: "after-tree".into(),
        changes: BTreeSet::from([FileChange {
            kind: FileChangeKind::Renamed,
            before_path: Some(before.into()),
            after_path: Some(after.into()),
            before_content_id: Some("before-file".into()),
            after_content_id: Some("after-file".into()),
        }]),
    }
}

fn align(
    before_index: &TypeScriptBindingIndex,
    before: &TypeScriptLoweringResult,
    after_index: &TypeScriptBindingIndex,
    after: &TypeScriptLoweringResult,
    diff: &RepositoryDiff,
    counterpart: Option<&BindingId>,
) -> Result<AlignmentResult, AlignmentError> {
    align_procedures(
        &before.procedure,
        &after.procedure,
        &alignable_bindings(before_index, &before.procedure),
        &alignable_bindings(after_index, &after.procedure),
        diff,
        &before.selected_binding,
        counterpart,
    )
}

fn writes(result: &TypeScriptLoweringResult) -> Vec<NodeId> {
    result
        .procedure
        .nodes
        .values()
        .filter(|node| matches!(node.operation, Operation::Write { .. }))
        .map(|node| node.id.clone())
        .collect()
}

fn paired_node<'a>(result: &'a AlignmentResult, before: &NodeId) -> &'a Alignment {
    result
        .nodes
        .iter()
        .find(|alignment| alignment.before.as_ref() == Some(before))
        .unwrap()
}

#[test]
fn ifds_k011_added_and_removed_declarations_are_not_changed_operations() {
    let path = "src/a.ts";
    let (before_index, before) = lower(
        SnapshotSide::Before,
        path,
        "function example() {\n  let x = 1;\n  let y = 2;\n  let z = x - y - 2;\n  return z;\n}\n",
        "x",
        None,
    );
    let (after_index, after) = lower(
        SnapshotSide::After,
        path,
        "function example() {\n  let a = 4;\n  let x = a - 1;\n  let z = x + a - 6;\n  return z;\n}\n",
        "x",
        None,
    );
    let result = align(
        &before_index,
        &before,
        &after_index,
        &after,
        &modified_diff(path),
        Some(&after.selected_binding),
    )
    .unwrap();
    let old_y = before_index
        .bindings
        .iter()
        .find(|binding| binding.name == "y")
        .unwrap();
    let new_a = after_index
        .bindings
        .iter()
        .find(|binding| binding.name == "a")
        .unwrap();
    let old_write = declaration_write(&before.procedure, &old_y.id, &old_y.declaration).unwrap();
    let new_write = declaration_write(&after.procedure, &new_a.id, &new_a.declaration).unwrap();
    assert_eq!(paired_node(&result, &old_write).after, None);
    assert!(
        result
            .nodes
            .iter()
            .any(|node| node.before.is_none() && node.after.as_ref() == Some(&new_write))
    );
}

#[test]
fn ifds_k011_declaration_write_with_inserted_input() {
    let path = "src/a.ts";
    let (before_index, before) = lower(
        SnapshotSide::Before,
        path,
        "function f() { let x = 1; let y = x + 2; return y; }",
        "x",
        None,
    );
    let (after_index, after) = lower(
        SnapshotSide::After,
        path,
        "function f() { let a = 5; let x = a / 6; let y = x + 2; return y; }",
        "x",
        None,
    );
    let result = align(
        &before_index,
        &before,
        &after_index,
        &after,
        &modified_diff(path),
        Some(&after.selected_binding),
    )
    .unwrap();
    assert_eq!(
        paired_node(&result, &writes(&before)[0]).after,
        Some(writes(&after)[1].clone())
    );
}

#[test]
fn ifds_k011_repeated_literal_keeps_unchanged_span() {
    let path = "src/a.ts";
    let (before_index, before) = lower(
        SnapshotSide::Before,
        path,
        "function f() { let x = 1; let y = x + 1; return y; }",
        "x",
        None,
    );
    let (after_index, after) = lower(
        SnapshotSide::After,
        path,
        "function f() { let x = 1; let y = x; return y; }",
        "x",
        None,
    );
    let result = align(
        &before_index,
        &before,
        &after_index,
        &after,
        &modified_diff(path),
        Some(&after.selected_binding),
    )
    .unwrap();
    let before_literal = before
        .procedure
        .nodes
        .values()
        .find(|node| {
            matches!(node.operation, Operation::Literal { .. })
                && node.span.as_ref().is_some_and(|span| span.byte_start == 23)
        })
        .unwrap();
    let after_literal = after
        .procedure
        .nodes
        .values()
        .find(|node| {
            matches!(node.operation, Operation::Literal { .. })
                && node.span.as_ref().is_some_and(|span| span.byte_start == 23)
        })
        .unwrap();
    assert_eq!(
        paired_node(&result, &before_literal.id).after,
        Some(after_literal.id.clone())
    );
    assert!(result.ambiguities.is_empty());
}

#[test]
fn ifds_k011_initializer_identity() {
    let path = "src/a.ts";
    let (before_index, before) = lower(
        SnapshotSide::Before,
        path,
        "function f() { let x = 1; return x; }",
        "x",
        None,
    );
    let (after_index, after) = lower(
        SnapshotSide::After,
        path,
        "function f() { let x = 2; return x; }",
        "x",
        None,
    );
    let result = align(
        &before_index,
        &before,
        &after_index,
        &after,
        &modified_diff(path),
        None,
    )
    .unwrap();

    assert!(result.bindings.iter().any(|binding| {
        binding.before.as_ref() == Some(&before.selected_binding)
            && binding.after.as_ref() == Some(&after.selected_binding)
    }));
    let before_write = writes(&before)[0].clone();
    let alignment = paired_node(&result, &before_write);
    assert!(alignment.after.is_some());
    assert_ne!(
        result.before_fingerprints[&before_write],
        result.after_fingerprints[alignment.after.as_ref().unwrap()]
    );
}

#[test]
fn ifds_k011_trivia_and_lines() {
    let path = "src/a.ts";
    let (before_index, before) = lower(
        SnapshotSide::Before,
        path,
        "function f() { let x = 1; return x; }",
        "x",
        None,
    );
    let (after_index, after) = lower(
        SnapshotSide::After,
        path,
        "// heading\nfunction f() {\n  let x = 1; // stable\n  return x;\n}",
        "x",
        None,
    );
    let result = align(
        &before_index,
        &before,
        &after_index,
        &after,
        &modified_diff(path),
        None,
    )
    .unwrap();
    for alignment in result.nodes.iter().filter(|alignment| {
        alignment.before.is_some()
            && alignment.after.is_some()
            && before.procedure.nodes[alignment.before.as_ref().unwrap()]
                .span
                .is_some()
    }) {
        assert_eq!(
            result.before_fingerprints[alignment.before.as_ref().unwrap()],
            result.after_fingerprints[alignment.after.as_ref().unwrap()]
        );
    }
    let before_write = writes(&before)[0].clone();
    let after_write = paired_node(&result, &before_write).after.as_ref().unwrap();
    assert_ne!(
        before.procedure.nodes[&before_write].span,
        after.procedure.nodes[after_write].span
    );
}

#[test]
fn ifds_k011_rename_mapping() {
    let (before_index, before) = lower(SnapshotSide::Before, "src/old.ts", "let x = 1;", "x", None);
    let (after_index, after) = lower(SnapshotSide::After, "src/new.ts", "let x = 1;", "x", None);
    let valid = align(
        &before_index,
        &before,
        &after_index,
        &after,
        &renamed_diff("src/old.ts", "src/new.ts"),
        None,
    )
    .unwrap();
    assert!(valid.bindings.iter().any(|binding| {
        binding.before.as_ref() == Some(&before.selected_binding)
            && binding.after.as_ref() == Some(&after.selected_binding)
    }));

    let invalid = align(
        &before_index,
        &before,
        &after_index,
        &after,
        &renamed_diff("src/other.ts", "src/new.ts"),
        None,
    )
    .unwrap();
    assert!(!invalid.bindings.iter().any(|binding| {
        binding.before.as_ref() == Some(&before.selected_binding) && binding.after.is_some()
    }));
}

#[test]
fn ifds_k011_retarget_operation() {
    let path = "src/a.ts";
    let before_source = "function f() { let x = 0; let y = 0; x = 2; return x; }";
    let after_source = "function f() { let x = 0; let y = 0; y = 2; return x; }";
    let (before_index, before) = lower(SnapshotSide::Before, path, before_source, "x", None);
    let (after_index, after) = lower(SnapshotSide::After, path, after_source, "x", None);
    let result = align(
        &before_index,
        &before,
        &after_index,
        &after,
        &modified_diff(path),
        None,
    )
    .unwrap();
    let retargeted = writes(&before).last().unwrap().clone();
    let alignment = paired_node(&result, &retargeted);
    assert!(alignment.after.is_some());
    assert_ne!(
        result.before_fingerprints[&retargeted],
        result.after_fingerprints[alignment.after.as_ref().unwrap()]
    );
}

#[test]
fn ifds_k011_moved_operation_identity() {
    let path = "src/a.ts";
    let (before_index, before) = lower(
        SnapshotSide::Before,
        path,
        "function f() { let x = 0; x = 1; x = 2; return x; }",
        "x",
        None,
    );
    let (after_index, after) = lower(
        SnapshotSide::After,
        path,
        "function f() { let x = 0; x = 2; x = 1; return x; }",
        "x",
        None,
    );
    let result = align(
        &before_index,
        &before,
        &after_index,
        &after,
        &modified_diff(path),
        None,
    )
    .unwrap();
    let moved = writes(&before)[1].clone();
    let alignment = paired_node(&result, &moved);
    let after_id = alignment.after.as_ref().unwrap();
    assert_eq!(
        result.before_fingerprints[&moved],
        result.after_fingerprints[after_id]
    );
    assert_ne!(moved.local, after_id.local);
    assert!(!result.nodes.iter().any(|candidate| {
        candidate.before.as_ref() == Some(&moved) && candidate.after.is_none()
    }));
}

#[test]
fn ifds_k011_repeated_call_ambiguity() {
    let path = "src/a.ts";
    let (before_index, before) = lower(
        SnapshotSide::Before,
        path,
        "function f() { let x = 0; foo(); foo(); return x; }",
        "x",
        None,
    );
    let (after_index, after) = lower(
        SnapshotSide::After,
        path,
        "function f() { let x = 0; foo(); foo(); foo(); return x; }",
        "x",
        None,
    );
    let result = align(
        &before_index,
        &before,
        &after_index,
        &after,
        &modified_diff(path),
        None,
    )
    .unwrap();
    assert!(!result.ambiguities.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::AmbiguousMatch)
    );
    let ambiguous = ambiguous_entities(&result.ambiguities);
    assert!(!result.nodes.iter().any(|alignment| {
        alignment.after.is_none()
            && alignment
                .before
                .as_ref()
                .is_some_and(|id| ambiguous.contains(&AlignmentEntity::Node(id.clone())))
    }));
}

#[test]
fn ifds_k011_explicit_counterpart() {
    let path = "src/a.ts";
    let (before_index, before) = lower(
        SnapshotSide::Before,
        path,
        "function f() { let x = 1; return x; }",
        "x",
        None,
    );
    let (after_index, after) = lower(
        SnapshotSide::After,
        path,
        "function f() { const renamed = 1; return renamed; }",
        "renamed",
        None,
    );
    let result = align(
        &before_index,
        &before,
        &after_index,
        &after,
        &modified_diff(path),
        Some(&after.selected_binding),
    )
    .unwrap();
    assert!(result.bindings.iter().any(|binding| {
        binding.before.as_ref() == Some(&before.selected_binding)
            && binding.after.as_ref() == Some(&after.selected_binding)
            && binding.evidence == "explicit counterpart"
    }));

    let (other_index, other) = lower(
        SnapshotSide::After,
        path,
        "function g() { let x = 1; return x; }",
        "x",
        None,
    );
    let error = align(
        &before_index,
        &before,
        &other_index,
        &other,
        &modified_diff(path),
        Some(&other.selected_binding),
    )
    .unwrap_err();
    assert!(matches!(error, AlignmentError::IncompatibleCounterpart(_)));

    let stale = BindingId::new(snapshot(SnapshotSide::After), u64::MAX);
    assert!(matches!(
        align(
            &before_index,
            &before,
            &after_index,
            &after,
            &modified_diff(path),
            Some(&stale)
        ),
        Err(AlignmentError::MissingCounterpart(_))
    ));
}

#[test]
fn ifds_k011_local_parameter_transition() {
    let path = "src/a.ts";
    let old = "function f() { let x = 1; return x; }";
    let new = "function f(x: number) { return x; }";
    for (before_source, after_source) in [(old, new), (new, old)] {
        let (before_index, before) = lower(SnapshotSide::Before, path, before_source, "x", None);
        let (after_index, after) = lower(SnapshotSide::After, path, after_source, "x", None);
        for counterpart in [None, Some(&after.selected_binding)] {
            let result = align(
                &before_index,
                &before,
                &after_index,
                &after,
                &modified_diff(path),
                counterpart,
            )
            .unwrap();
            assert!(result.bindings.iter().any(|binding| {
                binding.before.as_ref() == Some(&before.selected_binding)
                    && binding.after.as_ref() == Some(&after.selected_binding)
            }));
            let local_write = if before_source == old {
                writes(&before)[0].clone()
            } else {
                writes(&after)[0].clone()
            };
            assert!(result.nodes.iter().any(|node| {
                if before_source == old {
                    node.before.as_ref() == Some(&local_write) && node.after.is_none()
                } else {
                    node.before.is_none() && node.after.as_ref() == Some(&local_write)
                }
            }));
        }
    }
}

#[test]
fn ifds_k011_one_sided_binding() {
    let path = "src/a.ts";
    let (before_index, before) = lower(
        SnapshotSide::Before,
        path,
        "function f() { let x = 1; return x; }",
        "x",
        Some(BindingKind::Let),
    );
    let (after_index, after) = lower(
        SnapshotSide::After,
        path,
        "function f(y: number) { return y; }",
        "y",
        Some(BindingKind::Parameter),
    );
    let result = align(
        &before_index,
        &before,
        &after_index,
        &after,
        &modified_diff(path),
        None,
    )
    .unwrap();
    let old = result
        .bindings
        .iter()
        .find(|binding| binding.before.as_ref() == Some(&before.selected_binding))
        .unwrap();
    assert!(old.after.is_none());
    let new = result
        .bindings
        .iter()
        .find(|binding| binding.after.as_ref() == Some(&after.selected_binding))
        .unwrap();
    assert!(new.before.is_none());
    assert_ne!(old.logical, new.logical);
}
