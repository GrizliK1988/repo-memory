use super::*;
use crate::ifds::ir::{
    ComputeInputRole, LiteralKind, Operation, PrimitiveOperator, UnknownEffectKind,
};
use crate::ifds::model::{SnapshotSide, SourceSpan};

fn snapshot() -> SnapshotId {
    SnapshotId {
        side: SnapshotSide::Before,
        revision: "0123456789abcdef0123456789abcdef01234567".into(),
        content_id: "content".into(),
    }
}

fn span_of(path: &str, source: &str, needle: &str, occurrence: usize) -> SourceSpan {
    let (start, _) = source
        .match_indices(needle)
        .nth(occurrence)
        .expect("test occurrence must exist");
    SourceSpan {
        path: path.into(),
        byte_start: start as u64,
        byte_end: (start + needle.len()) as u64,
        start_line: source[..start].lines().count() as u32,
        end_line: source[..start].lines().count() as u32,
    }
}

fn selector(path: &str, source: &str, needle: &str, occurrence: usize) -> BindingSelector {
    BindingSelector {
        snapshot: snapshot(),
        declaration: span_of(path, source, needle, occurrence),
        expected_name: None,
        expected_enclosing_symbol: None,
    }
}

fn binding_named<'a>(
    index: &'a TypeScriptBindingIndex,
    name: &str,
    start: u64,
) -> &'a LexicalBinding {
    index
        .bindings
        .iter()
        .find(|binding| binding.name == name && binding.declaration.byte_start == start)
        .expect("binding must exist")
}

fn lower_named(source: &str, name: &str) -> TypeScriptLoweringResult {
    let index = index_bindings(snapshot(), "main.ts", source).unwrap();
    let binding = index
        .bindings
        .iter()
        .find(|binding| binding.name == name)
        .unwrap();
    lower_containing_procedure(source, &index, &binding.id).unwrap()
}

fn operations(result: &TypeScriptLoweringResult) -> Vec<&Operation> {
    result
        .procedure
        .nodes
        .values()
        .map(|node| &node.operation)
        .collect()
}

#[test]
fn ifds_k006_initializer_order() {
    let result = lower_named(
        "function f(input: number) { let x = input + 1; return x; }",
        "x",
    );
    let operations = operations(&result);
    assert!(matches!(operations[0], Operation::Entry));
    assert!(matches!(operations[1], Operation::Read { .. }));
    assert!(matches!(
        operations[2],
        Operation::Literal {
            literal_kind: LiteralKind::Number,
            ..
        }
    ));
    assert!(matches!(
        operations[3],
        Operation::Compute {
            operator: PrimitiveOperator::Add,
            ..
        }
    ));
    assert!(matches!(operations[4], Operation::Write { .. }));
    assert!(matches!(operations[5], Operation::Read { .. }));
    assert!(matches!(operations[6], Operation::Return { .. }));
    assert_eq!(result.procedure.parameters.len(), 1);
    assert_eq!(result.procedure.parameters[0].index, 0);
    result.procedure.validate().unwrap();
}

#[test]
fn ifds_k006_self_assignment_order() {
    let result = lower_named("function f() { let x = 0; x = x + 1; return x; }", "x");
    let operations = operations(&result);
    let writes: Vec<_> = operations
        .iter()
        .enumerate()
        .filter(|(_, op)| matches!(op, Operation::Write { .. }))
        .collect();
    assert_eq!(writes.len(), 2);
    let second_write = writes[1].0;
    assert!(matches!(
        operations[second_write - 3],
        Operation::Read { .. }
    ));
    assert!(matches!(
        operations[second_write - 1],
        Operation::Compute {
            operator: PrimitiveOperator::Add,
            ..
        }
    ));
}

#[test]
fn ifds_k006_multiple_operands() {
    let result = lower_named(
        "function f(a: number, b: number) { let y = a + b; return `${a}-${b}`; }",
        "y",
    );
    let operations = operations(&result);
    let add = operations
        .iter()
        .find_map(|operation| match operation {
            Operation::Compute {
                operator: PrimitiveOperator::Add,
                inputs,
                ..
            } => Some(inputs),
            _ => None,
        })
        .unwrap();
    assert_eq!(add.len(), 2);
    assert_eq!(add[0].role, ComputeInputRole::Operand { index: 0 });
    assert_eq!(add[1].role, ComputeInputRole::Operand { index: 1 });
    let template = operations
        .iter()
        .find_map(|operation| match operation {
            Operation::Compute {
                operator: PrimitiveOperator::Template,
                inputs,
                ..
            } => Some(inputs),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        template
            .iter()
            .filter(|input| matches!(input.role, ComputeInputRole::TemplateInterpolation { .. }))
            .count(),
        2
    );
    assert!(
        template
            .iter()
            .any(|input| matches!(input.role, ComputeInputRole::TemplateChunk { .. }))
    );
}

#[test]
fn ifds_k006_return_ends_path() {
    let result = lower_named("function f() { let x = 1; return x; x = 2; }", "x");
    let return_id = result
        .procedure
        .nodes
        .values()
        .find(|node| matches!(node.operation, Operation::Return { .. }))
        .unwrap()
        .id
        .clone();
    let later_write = result
        .procedure
        .nodes
        .values()
        .filter(|node| matches!(node.operation, Operation::Write { .. }))
        .next_back()
        .unwrap()
        .id
        .clone();
    assert!(
        !result
            .procedure
            .edges
            .iter()
            .any(|edge| edge.source == return_id && edge.target == later_write)
    );
    let mut reachable = BTreeSet::from([result.procedure.entry.clone()]);
    loop {
        let next: BTreeSet<_> = result
            .procedure
            .edges
            .iter()
            .filter(|edge| reachable.contains(&edge.source))
            .map(|edge| edge.target.clone())
            .collect();
        let old_len = reachable.len();
        reachable.extend(next);
        if reachable.len() == old_len {
            break;
        }
    }
    assert!(!reachable.contains(&later_write));
}

#[test]
fn ifds_k006_source_mapping() {
    let source = "function f(a: number) { let x = a + 1; return x; }";
    let result = lower_named(source, "x");
    let nodes: Vec<_> = result.procedure.nodes.values().collect();
    let read_a = nodes
        .iter()
        .find(|node| {
            matches!(node.operation, Operation::Read { .. })
                && node.span.as_ref().is_some_and(|span| {
                    &source[span.byte_start as usize..span.byte_end as usize] == "a"
                })
        })
        .unwrap();
    assert_eq!(
        read_a.span.as_ref().unwrap(),
        &span_of("main.ts", source, "a", 1)
    );
    let compute = nodes
        .iter()
        .find(|node| {
            matches!(
                node.operation,
                Operation::Compute {
                    operator: PrimitiveOperator::Add,
                    ..
                }
            )
        })
        .unwrap();
    assert_eq!(
        &source[compute.span.as_ref().unwrap().byte_start as usize
            ..compute.span.as_ref().unwrap().byte_end as usize],
        "a + 1"
    );
    let write = nodes
        .iter()
        .find(|node| matches!(node.operation, Operation::Write { .. }))
        .unwrap();
    assert_eq!(
        &source[write.span.as_ref().unwrap().byte_start as usize
            ..write.span.as_ref().unwrap().byte_end as usize],
        "x = a + 1"
    );
}

#[test]
fn ifds_k006_unsupported_effect() {
    let result = lower_named(
        "function f(input: number, callback: unknown) { let x = callback(input); return x; }",
        "x",
    );
    let unknown = result
        .procedure
        .nodes
        .values()
        .find(|node| matches!(node.operation, Operation::UnknownEffect { .. }))
        .unwrap();
    assert!(matches!(
        unknown.operation,
        Operation::UnknownEffect {
            effect_kind: UnknownEffectKind::Value,
            result: Some(_),
            ..
        }
    ));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.frontier.as_ref() == Some(&unknown.id))
    );
    assert!(
        result
            .procedure
            .edges
            .iter()
            .any(|edge| edge.source == unknown.id)
    );
}

#[test]
fn ifds_k006_control_frontier() {
    let result = lower_named(
        "function f(flag: boolean) { let x = 1; if (flag) { return x; } x = 2; }",
        "x",
    );
    let branch = result
        .procedure
        .nodes
        .values()
        .find(|node| matches!(node.operation, Operation::Branch { .. }))
        .unwrap();
    assert_eq!(
        result
            .procedure
            .edges
            .iter()
            .filter(|edge| edge.source == branch.id)
            .count(),
        2
    );
}

#[test]
fn ifds_k006_module_export_boundary() {
    let result = lower_named("const result = 1; export { result };", "result");
    let operations = operations(&result);
    assert!(
        operations
            .iter()
            .any(|operation| matches!(operation, Operation::Write { .. }))
    );
    assert!(
        operations
            .iter()
            .any(|operation| matches!(operation, Operation::Read { .. }))
    );
    assert!(operations.iter().any(|operation| matches!(
        operation,
        Operation::UnknownEffect {
            effect_kind: UnknownEffectKind::Value,
            result: None,
            ..
        }
    )));
}

#[test]
fn ifds_k005_grammar_selection() {
    let ts = index_bindings(snapshot(), "src/value.ts", "const value = 1;").unwrap();
    assert_eq!(ts.grammar, TypeScriptGrammar::TypeScript);
    assert!(ts.diagnostics.is_empty());

    let tsx_source = "const value = 1; const view = <div>{value}</div>;";
    let tsx = index_bindings(snapshot(), "src/view.tsx", tsx_source).unwrap();
    assert_eq!(tsx.grammar, TypeScriptGrammar::Tsx);
    assert!(tsx.bindings.iter().any(|binding| binding.name == "view"));
    assert!(tsx.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnsupportedSyntax
            && diagnostic.message.contains("JSX parsed successfully")
    }));
    assert!(
        !tsx.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::ParseError)
    );
}

#[test]
fn ifds_k005_shadowed_names() {
    let source = r#"let x = 0;
x;
function example(x: number) {
  const fromParameter = x;
  {
    let x = 1;
    const fromBlock = x;
  }
  return x;
}
"#;
    let index = index_bindings(snapshot(), "main.ts", source).unwrap();
    assert!(index.diagnostics.is_empty(), "{:?}", index.diagnostics);

    let declarations: Vec<_> = index
        .bindings
        .iter()
        .filter(|binding| binding.name == "x")
        .collect();
    assert_eq!(declarations.len(), 3);
    assert_ne!(declarations[0].id, declarations[1].id);
    assert_ne!(declarations[1].id, declarations[2].id);

    let reference_bindings: Vec<_> = index
        .references
        .iter()
        .filter(|reference| reference.name == "x")
        .map(|reference| reference.binding.clone().unwrap())
        .collect();
    assert_eq!(reference_bindings.len(), 4);
    assert_eq!(reference_bindings[0], declarations[0].id);
    assert_eq!(reference_bindings[1], declarations[1].id);
    assert_eq!(reference_bindings[2], declarations[2].id);
    assert_eq!(reference_bindings[3], declarations[1].id);
}

#[test]
fn ifds_k005_declaration_not_reference() {
    let source = "function example() { let value = 1; return value; }";
    let index = index_bindings(snapshot(), "main.ts", source).unwrap();

    let declaration = selector("main.ts", source, "value", 0);
    assert_eq!(index.resolve_selector(&declaration).unwrap().name, "value");

    let reference = selector("main.ts", source, "value", 1);
    let error = index.resolve_selector(&reference).unwrap_err();
    assert!(error.to_string().contains("reference occurrence"));
}

#[test]
fn ifds_k005_stale_selector_assertions() {
    let source = "function example() { const value = 1; return value; }";
    let index = index_bindings(snapshot(), "main.ts", source).unwrap();
    let base = selector("main.ts", source, "value", 0);
    let resolved = index.resolve_selector(&base).unwrap().id.clone();

    let mut matching = base.clone();
    matching.expected_name = Some("value".into());
    matching.expected_enclosing_symbol = Some("example".into());
    assert_eq!(index.resolve_selector(&matching).unwrap().id, resolved);

    let mut stale_name = matching.clone();
    stale_name.expected_name = Some("other".into());
    assert!(
        index
            .resolve_selector(&stale_name)
            .unwrap_err()
            .to_string()
            .contains("stale selector")
    );

    let mut stale_owner = matching;
    stale_owner.expected_enclosing_symbol = Some("other".into());
    assert!(
        index
            .resolve_selector(&stale_owner)
            .unwrap_err()
            .to_string()
            .contains("stale selector")
    );
}

#[test]
fn ifds_k005_unicode_identifier() {
    let source = "function пример(вход: number) {\n  const café = вход;\n  return café;\n}\n";
    let index = index_bindings(snapshot(), "unicode.ts", source).unwrap();
    assert!(index.diagnostics.is_empty(), "{:?}", index.diagnostics);

    let declaration_span = span_of("unicode.ts", source, "café", 0);
    let binding = binding_named(&index, "café", declaration_span.byte_start);
    assert_eq!(binding.declaration, declaration_span);
    assert_eq!(binding.declaration.start_line, 2);
    assert_eq!(
        binding.declaration.byte_end - binding.declaration.byte_start,
        "café".len() as u64
    );

    let reference = index
        .references
        .iter()
        .find(|reference| reference.name == "café")
        .unwrap();
    assert_eq!(reference.span, span_of("unicode.ts", source, "café", 1));
    assert_eq!(reference.span.start_line, 3);
    assert_eq!(reference.binding.as_ref(), Some(&binding.id));

    let parameter = index
        .bindings
        .iter()
        .find(|candidate| candidate.name == "вход")
        .unwrap();
    let parameter_read = index
        .references
        .iter()
        .find(|candidate| candidate.name == "вход")
        .unwrap();
    assert_eq!(parameter_read.binding.as_ref(), Some(&parameter.id));
}

#[test]
fn ifds_k005_separate_functions() {
    let source = r#"function first() {
  const x = 1;
  return x;
}
function second() {
  const x = 2;
  return x;
}
"#;
    let index = index_bindings(snapshot(), "main.ts", source).unwrap();
    assert!(index.diagnostics.is_empty(), "{:?}", index.diagnostics);
    let bindings: Vec<_> = index
        .bindings
        .iter()
        .filter(|binding| binding.name == "x")
        .collect();
    let reads: Vec<_> = index
        .references
        .iter()
        .filter(|reference| reference.name == "x")
        .collect();
    assert_eq!(bindings.len(), 2);
    assert_eq!(reads.len(), 2);
    assert_eq!(reads[0].binding.as_ref(), Some(&bindings[0].id));
    assert_eq!(reads[1].binding.as_ref(), Some(&bindings[1].id));
    assert_ne!(reads[0].binding, reads[1].binding);
}

#[test]
fn ifds_k005_unsupported_or_invalid() {
    let unsupported = r#"function example({input}: {input: number}, ...rest: number[]) {
  const {value} = input;
  var hoisted = value;
  return missing;
}
"#;
    let index = index_bindings(snapshot(), "main.ts", unsupported).unwrap();
    assert!(index.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnsupportedSyntax
            && diagnostic.message.contains("parameter binding")
    }));
    assert!(index.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnsupportedSyntax
            && diagnostic.message.contains("destructuring")
    }));
    assert!(index.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnsupportedSyntax
            && diagnostic.message.contains("hoisting")
    }));
    assert!(index.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnresolvedBinding
            && diagnostic.message.contains("missing")
    }));

    let invalid = index_bindings(
        snapshot(),
        "broken.ts",
        "function broken( { const value = ;",
    )
    .unwrap();
    assert!(invalid.bindings.is_empty());
    assert!(invalid.references.is_empty());
    assert!(
        invalid
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::ParseError)
    );
}
