use super::*;
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
