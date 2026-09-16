//! Compare declarations in two TSX snapshots after validating a single-file unified diff.
//!
//! Symbols are matched by their declaration ancestry, name, and occurrence (for overloads).
//! Renames are removals plus additions. Moving an unchanged declaration within its scope
//! does not count as a change. Any edit inside a declaration, including formatting or
//! comments, counts as a change; enclosing declarations are reported as well.
//! This is syntactic analysis, not TypeScript type inference or reference resolution.

use std::collections::BTreeMap;
use std::fmt;

use tree_sitter::{Node, Parser};

mod jsx;
pub use crate::changes::{ChangeKind, ChangeValue, ChildChange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Variable,
    Class,
    Method,
    Property,
    Interface,
    TypeAlias,
    Enum,
    Namespace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    /// Names of enclosing declarations followed by this name, separated by `::`.
    pub qualified_name: String,
    pub kind: SymbolKind,
    /// Explicit variable/property type or function return type, without the leading `:`.
    pub type_annotation: Option<String>,
    /// One-based, inclusive line numbers in the corresponding snapshot.
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolChange {
    pub kind: ChangeKind,
    /// Present for removed and changed symbols.
    pub before: Option<Symbol>,
    /// Present for added and changed symbols.
    pub after: Option<Symbol>,
    /// Changes within child constructs of this declaration. The TSX adapter
    /// currently reports JSX inputs; details are not an exhaustive semantic diff.
    pub child_changes: Vec<ChildChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffError {
    InvalidDiff(String),
    ContentMismatch,
    InvalidSyntax { snapshot: &'static str },
    Parser(String),
}

impl fmt::Display for DiffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDiff(reason) => write!(f, "invalid single-file unified diff: {reason}"),
            Self::ContentMismatch => {
                write!(f, "diff does not transform original into modified contents")
            }
            Self::InvalidSyntax { snapshot } => {
                write!(f, "invalid TSX syntax in {snapshot} contents")
            }
            Self::Parser(reason) => write!(f, "Tree-sitter parser error: {reason}"),
        }
    }
}

impl std::error::Error for DiffError {}

/// Calculate added, removed, and changed TSX declarations, sorted by qualified name
/// and occurrence. Accepts ordinary Git unified diffs (including `--unified=0`) or
/// bare unified patches. The patch must describe the complete change to one file.
/// An empty diff is valid only when both snapshots are identical.
///
/// Named functions, function-valued variables, variables (including destructuring),
/// classes, methods, properties, interfaces, type aliases, enums, and namespaces
/// are supported. Anonymous default-export functions/classes use the name `default`.
/// Imports, parameters, enum members, and references are not separate symbols.
/// Anonymous lexical blocks do not introduce a distinct matching scope; duplicate
/// names within the same declaration ancestry are matched in source order.
/// JSX prop details are attached to each changed declaration, including enclosing
/// declarations. They cover explicit props and inline property values; they are
/// not an exhaustive semantic diff (see the crate documentation for limitations).
///
/// # Errors
/// Returns an error for invalid/multi-file patches, mismatched snapshots, or TSX
/// syntax errors in either snapshot.
pub fn analyze_tsx_diff(
    git_diff: &str,
    original: &str,
    modified: &str,
) -> Result<Vec<SymbolChange>, DiffError> {
    validate_diff(git_diff, original, modified)?;
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
        .map_err(|error| DiffError::Parser(error.to_string()))?;
    let before = extract_symbols(&mut parser, original, "original")?;
    let mut after = extract_symbols(&mut parser, modified, "modified")?;
    let mut changes = BTreeMap::new();

    for (key, old) in before {
        match after.remove(&key) {
            Some(new)
                if old.fingerprint == new.fingerprint && old.symbol.kind == new.symbol.kind => {}
            Some(new) => {
                changes.insert(
                    key,
                    SymbolChange {
                        kind: ChangeKind::Changed,
                        child_changes: jsx::compare(&old.jsx, &new.jsx),
                        before: Some(old.symbol),
                        after: Some(new.symbol),
                    },
                );
            }
            None => {
                changes.insert(
                    key,
                    SymbolChange {
                        kind: ChangeKind::Removed,
                        child_changes: jsx::compare(&old.jsx, &BTreeMap::new()),
                        before: Some(old.symbol),
                        after: None,
                    },
                );
            }
        }
    }
    for (key, new) in after {
        changes.insert(
            key,
            SymbolChange {
                kind: ChangeKind::Added,
                child_changes: jsx::compare(&BTreeMap::new(), &new.jsx),
                before: None,
                after: Some(new.symbol),
            },
        );
    }
    Ok(changes.into_values().collect())
}

fn validate_diff(diff: &str, original: &str, modified: &str) -> Result<(), DiffError> {
    if diff
        .lines()
        .filter(|line| line.starts_with("diff --git "))
        .count()
        > 1
    {
        return Err(DiffError::InvalidDiff("expected exactly one file".into()));
    }
    if diff.lines().any(|line| {
        line.starts_with("GIT binary patch")
            || line.starts_with("Binary files ")
            || line.starts_with("diff --cc ")
            || line.starts_with("diff --combined ")
            || line.starts_with("@@@")
    }) {
        return Err(DiffError::InvalidDiff(
            "binary and combined diffs are unsupported".into(),
        ));
    }
    if diff
        .lines()
        .any(|line| line.starts_with("@@") && !line.starts_with("@@ "))
    {
        return Err(DiffError::InvalidDiff("malformed hunk header".into()));
    }
    if !diff.trim().is_empty()
        && !diff
            .lines()
            .any(|line| line.starts_with("@@ ") || line.starts_with("diff --git "))
        && !diff.starts_with("--- ")
    {
        return Err(DiffError::InvalidDiff(
            "missing unified diff headers".into(),
        ));
    }
    let patch =
        diffy::Patch::from_str(diff).map_err(|error| DiffError::InvalidDiff(error.to_string()))?;
    let applied = diffy::apply(original, &patch).map_err(|_| DiffError::ContentMismatch)?;
    if applied != modified {
        return Err(DiffError::ContentMismatch);
    }
    Ok(())
}

type SymbolKey = (String, usize);

struct Declaration {
    symbol: Symbol,
    fingerprint: String,
    jsx: jsx::Elements,
}

fn extract_symbols(
    parser: &mut Parser,
    source: &str,
    snapshot: &'static str,
) -> Result<BTreeMap<SymbolKey, Declaration>, DiffError> {
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| DiffError::Parser("parsing cancelled".into()))?;
    if tree.root_node().has_error() {
        return Err(DiffError::InvalidSyntax { snapshot });
    }
    let mut symbols = BTreeMap::new();
    let mut occurrences = BTreeMap::new();
    collect(tree.root_node(), source, "", &mut symbols, &mut occurrences);
    Ok(symbols)
}

fn collect(
    node: Node<'_>,
    source: &str,
    scope: &str,
    symbols: &mut BTreeMap<SymbolKey, Declaration>,
    occurrences: &mut BTreeMap<String, usize>,
) {
    let mut child_scope = scope.to_owned();
    if let Some(kind) = symbol_kind(node) {
        let names = symbol_names(node, source);
        for name in &names {
            let qualified_name = if scope.is_empty() {
                name.clone()
            } else {
                format!("{scope}::{name}")
            };
            let occurrence = occurrences.entry(qualified_name.clone()).or_default();
            let key = (qualified_name.clone(), *occurrence);
            *occurrence += 1;
            let annotation_node = node
                .child_by_field_name("type")
                .or_else(|| node.child_by_field_name("return_type"))
                .or_else(|| {
                    node.child_by_field_name("value")?
                        .child_by_field_name("return_type")
                });
            let symbol = Symbol {
                name: name.clone(),
                qualified_name: qualified_name.clone(),
                kind,
                type_annotation: annotation_node.map(|n| {
                    source[n.byte_range()]
                        .trim_start_matches(':')
                        .trim()
                        .to_owned()
                }),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + usize::from(node.end_position().column > 0),
            };
            symbols.insert(
                key,
                Declaration {
                    symbol,
                    fingerprint: fingerprint(node, source),
                    jsx: jsx::extract(node, source),
                },
            );
            if names.len() == 1 {
                child_scope = qualified_name;
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect(child, source, &child_scope, symbols, occurrences);
    }
}

fn symbol_kind(node: Node<'_>) -> Option<SymbolKind> {
    Some(match node.kind() {
        "function_declaration" | "generator_function_declaration" | "function_signature" => {
            SymbolKind::Function
        }
        "variable_declarator" => match node.child_by_field_name("value").map(|n| n.kind()) {
            Some("arrow_function" | "function_expression" | "generator_function") => {
                SymbolKind::Function
            }
            _ => SymbolKind::Variable,
        },
        "class_declaration" | "abstract_class_declaration" => SymbolKind::Class,
        "method_definition" | "method_signature" | "abstract_method_signature" => {
            SymbolKind::Method
        }
        "public_field_definition" | "property_signature" => SymbolKind::Property,
        "interface_declaration" => SymbolKind::Interface,
        "type_alias_declaration" => SymbolKind::TypeAlias,
        "enum_declaration" => SymbolKind::Enum,
        "internal_module" | "module" => SymbolKind::Namespace,
        "function_expression" | "generator_function" if is_default_export(node) => {
            SymbolKind::Function
        }
        "class" if is_default_export(node) => SymbolKind::Class,
        _ => return None,
    })
}

fn is_default_export(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|parent| parent.kind() == "export_statement")
}

fn symbol_names(node: Node<'_>, source: &str) -> Vec<String> {
    let Some(name) = node.child_by_field_name("name") else {
        return if is_default_export(node) {
            vec!["default".into()]
        } else {
            vec![]
        };
    };
    if node.kind() == "variable_declarator" {
        let mut names = Vec::new();
        binding_names(name, source, &mut names);
        names
    } else {
        vec![source[name.byte_range()].to_owned()]
    }
}

fn binding_names(node: Node<'_>, source: &str, names: &mut Vec<String>) {
    match node.kind() {
        "identifier" | "shorthand_property_identifier_pattern" => {
            names.push(source[node.byte_range()].to_owned())
        }
        "pair_pattern" => {
            if let Some(value) = node.child_by_field_name("value") {
                binding_names(value, source, names);
            }
        }
        "assignment_pattern" | "object_assignment_pattern" => {
            if let Some(left) = node.child_by_field_name("left") {
                binding_names(left, source, names);
            }
        }
        "array_pattern" | "object_pattern" | "rest_pattern" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                binding_names(child, source, names);
            }
        }
        _ => {}
    }
}

// Include declaration modifiers (const/let, export, declare) without including
// sibling declarators: editing `const a = 1, b = 2` must not mark both as changed.
fn fingerprint(node: Node<'_>, source: &str) -> String {
    let mut text = source[node.byte_range()].to_owned();
    let mut current = node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "lexical_declaration" | "variable_declaration" => {
                let mut cursor = parent.walk();
                if let Some(first) = parent
                    .named_children(&mut cursor)
                    .find(|n| n.kind() == "variable_declarator")
                {
                    text.insert_str(0, &source[parent.start_byte()..first.start_byte()]);
                }
            }
            "export_statement" | "ambient_declaration" => {
                text.insert_str(0, &source[parent.start_byte()..current.start_byte()]);
            }
            _ => break,
        }
        current = parent;
    }
    text
}

#[cfg(test)]
mod tests;
