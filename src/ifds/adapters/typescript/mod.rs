//! TypeScript parsing and lexical binding for the stage-one scalar subset.

use crate::ifds::model::{
    BindingId, BindingSelector, Diagnostic, DiagnosticCode, InputError, SnapshotId, SourceSpan,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::Path;
use tree_sitter::{Node, Parser, Point};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeScriptGrammar {
    TypeScript,
    Tsx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Program,
    Function,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalScope {
    pub id: ScopeId,
    pub parent: Option<ScopeId>,
    pub kind: ScopeKind,
    pub span: SourceSpan,
    pub enclosing_symbol: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    Let,
    Const,
    Parameter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexicalBinding {
    pub id: BindingId,
    pub name: String,
    pub kind: BindingKind,
    pub declaration: SourceSpan,
    pub scope: ScopeId,
    pub enclosing_symbol: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingReference {
    pub name: String,
    pub span: SourceSpan,
    pub scope: ScopeId,
    pub binding: Option<BindingId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeScriptBindingIndex {
    pub snapshot: SnapshotId,
    pub path: String,
    pub grammar: TypeScriptGrammar,
    pub scopes: Vec<LexicalScope>,
    pub bindings: Vec<LexicalBinding>,
    pub references: Vec<BindingReference>,
    pub diagnostics: Vec<Diagnostic>,
}

impl TypeScriptBindingIndex {
    pub fn resolve_selector(
        &self,
        selector: &BindingSelector,
    ) -> Result<&LexicalBinding, InputError> {
        if selector.snapshot != self.snapshot {
            return Err(InputError::SnapshotMismatch(
                "selector snapshot does not match the binding index".into(),
            ));
        }
        if selector.declaration.path != self.path {
            return Err(InputError::InvalidSelector(format!(
                "selector path {:?} does not match indexed path {:?}",
                selector.declaration.path, self.path
            )));
        }

        let mut matches = self.bindings.iter().filter(|binding| {
            binding.declaration.byte_start == selector.declaration.byte_start
                && binding.declaration.byte_end == selector.declaration.byte_end
        });
        let Some(binding) = matches.next() else {
            let is_reference = self.references.iter().any(|reference| {
                reference.span.byte_start == selector.declaration.byte_start
                    && reference.span.byte_end == selector.declaration.byte_end
            });
            let reason = if is_reference {
                "selector points to a reference occurrence, not a declaration"
            } else {
                "selector does not identify a supported declaration"
            };
            return Err(InputError::InvalidSelector(reason.into()));
        };
        if matches.next().is_some() {
            return Err(InputError::InvalidSelector(
                "selector identifies multiple declarations".into(),
            ));
        }
        if let Some(expected) = &selector.expected_name
            && expected != &binding.name
        {
            return Err(InputError::InvalidSelector(format!(
                "stale selector: expected declaration name {expected:?}, found {:?}",
                binding.name
            )));
        }
        if let Some(expected) = &selector.expected_enclosing_symbol
            && binding.enclosing_symbol.as_ref() != Some(expected)
        {
            return Err(InputError::InvalidSelector(format!(
                "stale selector: expected enclosing symbol {expected:?}, found {:?}",
                binding.enclosing_symbol
            )));
        }
        Ok(binding)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeScriptAdapterError {
    UnsupportedFileKind(String),
    Parser(String),
}

impl fmt::Display for TypeScriptAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFileKind(path) => {
                write!(f, "unsupported TypeScript file kind for {path:?}")
            }
            Self::Parser(reason) => write!(f, "failed to initialize TypeScript parser: {reason}"),
        }
    }
}

impl std::error::Error for TypeScriptAdapterError {}

pub fn grammar_for_path(path: &str) -> Result<TypeScriptGrammar, TypeScriptAdapterError> {
    match Path::new(path).extension().and_then(|value| value.to_str()) {
        Some("ts") => Ok(TypeScriptGrammar::TypeScript),
        Some("tsx") => Ok(TypeScriptGrammar::Tsx),
        _ => Err(TypeScriptAdapterError::UnsupportedFileKind(path.into())),
    }
}

pub fn index_bindings(
    snapshot: SnapshotId,
    path: &str,
    source: &str,
) -> Result<TypeScriptBindingIndex, TypeScriptAdapterError> {
    let grammar = grammar_for_path(path)?;
    let mut parser = Parser::new();
    let language = match grammar {
        TypeScriptGrammar::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        TypeScriptGrammar::Tsx => tree_sitter_typescript::LANGUAGE_TSX,
    };
    parser
        .set_language(&language.into())
        .map_err(|error| TypeScriptAdapterError::Parser(error.to_string()))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| TypeScriptAdapterError::Parser("parser returned no tree".into()))?;
    let root = tree.root_node();

    let mut builder = IndexBuilder::new(snapshot, path, source, grammar, root);
    if root.has_error() {
        builder.collect_parse_errors(root);
        return Ok(builder.finish());
    }
    builder.collect_declarations(root, ScopeId(0), false);
    builder.collect_references(root, ScopeId(0), false);
    builder.resolve_references();
    Ok(builder.finish())
}

struct IndexBuilder<'a> {
    snapshot: SnapshotId,
    path: &'a str,
    source: &'a str,
    grammar: TypeScriptGrammar,
    scopes: Vec<LexicalScope>,
    bindings: Vec<LexicalBinding>,
    references: Vec<BindingReference>,
    diagnostics: Vec<Diagnostic>,
    scope_starts: HashMap<usize, ScopeId>,
    declarations: HashSet<usize>,
    ignored_identifiers: HashSet<usize>,
    names_by_scope: BTreeMap<ScopeId, BTreeMap<String, Vec<BindingId>>>,
    jsx_boundaries: HashSet<usize>,
}

impl<'a> IndexBuilder<'a> {
    fn new(
        snapshot: SnapshotId,
        path: &'a str,
        source: &'a str,
        grammar: TypeScriptGrammar,
        root: Node<'_>,
    ) -> Self {
        Self {
            snapshot,
            path,
            source,
            grammar,
            scopes: vec![LexicalScope {
                id: ScopeId(0),
                parent: None,
                kind: ScopeKind::Program,
                span: source_span(path, root),
                enclosing_symbol: None,
            }],
            bindings: Vec::new(),
            references: Vec::new(),
            diagnostics: Vec::new(),
            scope_starts: HashMap::new(),
            declarations: HashSet::new(),
            ignored_identifiers: HashSet::new(),
            names_by_scope: BTreeMap::new(),
            jsx_boundaries: HashSet::new(),
        }
    }

    fn finish(mut self) -> TypeScriptBindingIndex {
        self.diagnostics.sort();
        self.diagnostics.dedup();
        TypeScriptBindingIndex {
            snapshot: self.snapshot,
            path: self.path.into(),
            grammar: self.grammar,
            scopes: self.scopes,
            bindings: self.bindings,
            references: self.references,
            diagnostics: self.diagnostics,
        }
    }

    fn collect_parse_errors(&mut self, node: Node<'_>) {
        if node.is_error() || node.is_missing() {
            self.push_diagnostic(
                DiagnosticCode::ParseError,
                "TypeScript parse error; lexical bindings were not inferred",
                node,
            );
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.has_error() || child.is_error() || child.is_missing() {
                self.collect_parse_errors(child);
            }
        }
    }

    fn collect_declarations(&mut self, node: Node<'_>, scope: ScopeId, function_body: bool) {
        if is_jsx(node.kind()) {
            if self.jsx_boundaries.insert(node.id()) {
                self.push_diagnostic(
                    DiagnosticCode::UnsupportedSyntax,
                    "JSX parsed successfully but its execution semantics are not modeled",
                    node,
                );
            }
            return;
        }

        if is_function_like(node.kind()) {
            self.collect_function(node, scope);
            return;
        }

        let scope = if node.kind() == "statement_block" && !function_body {
            self.new_scope(node, scope, ScopeKind::Block, self.scope_symbol(scope))
        } else {
            scope
        };

        match node.kind() {
            "lexical_declaration" => self.collect_lexical_declaration(node, scope),
            "variable_declaration" => self.push_diagnostic(
                DiagnosticCode::UnsupportedSyntax,
                "var declarations and hoisting semantics are not supported",
                node,
            ),
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_declarations(child, scope, false);
        }
    }

    fn collect_function(&mut self, node: Node<'_>, parent: ScopeId) {
        let symbol = function_symbol(node, self.source);
        if let Some(name) = node.child_by_field_name("name") {
            self.ignore_identifier_tree(name);
        }
        let scope = self.new_scope(node, parent, ScopeKind::Function, symbol);

        if let Some(parameter) = node.child_by_field_name("parameter") {
            if parameter.kind() == "identifier" {
                self.add_binding(parameter, scope, BindingKind::Parameter);
            } else {
                self.unsupported_binding(parameter, "unsupported arrow parameter binding form");
            }
        }
        if let Some(parameters) = node.child_by_field_name("parameters") {
            let mut cursor = parameters.walk();
            for parameter in parameters.named_children(&mut cursor) {
                self.collect_parameter(parameter, scope);
            }
        }

        if let Some(body) = node.child_by_field_name("body") {
            self.collect_declarations(body, scope, true);
        }
    }

    fn collect_parameter(&mut self, parameter: Node<'_>, scope: ScopeId) {
        if parameter.kind() != "required_parameter" {
            self.unsupported_binding(parameter, "unsupported optional/default parameter form");
            return;
        }
        let name = parameter
            .child_by_field_name("name")
            .or_else(|| parameter.child_by_field_name("pattern"));
        match name {
            Some(name) if name.kind() == "identifier" => {
                self.add_binding(name, scope, BindingKind::Parameter)
            }
            Some(name) => self.unsupported_binding(name, "unsupported parameter binding form"),
            None => self.unsupported_binding(parameter, "unsupported parameter binding form"),
        }
    }

    fn collect_lexical_declaration(&mut self, declaration: Node<'_>, scope: ScopeId) {
        let kind = declaration
            .child_by_field_name("kind")
            .map(|node| node.kind());
        let binding_kind = match kind {
            Some("const") => BindingKind::Const,
            _ => BindingKind::Let,
        };
        let mut cursor = declaration.walk();
        for declarator in declaration.named_children(&mut cursor) {
            if declarator.kind() != "variable_declarator" {
                continue;
            }
            let Some(name) = declarator.child_by_field_name("name") else {
                continue;
            };
            if name.kind() == "identifier" {
                self.add_binding(name, scope, binding_kind);
            } else {
                self.unsupported_binding(name, "destructuring bindings are not supported");
            }
        }
    }

    fn add_binding(&mut self, node: Node<'_>, scope: ScopeId, kind: BindingKind) {
        self.declarations.insert(node.id());
        let name = node_text(node, self.source).to_owned();
        let id = BindingId::new(self.snapshot.clone(), self.bindings.len() as u64 + 1);
        self.names_by_scope
            .entry(scope)
            .or_default()
            .entry(name.clone())
            .or_default()
            .push(id.clone());
        self.bindings.push(LexicalBinding {
            id,
            name,
            kind,
            declaration: source_span(self.path, node),
            scope,
            enclosing_symbol: self.scope_symbol(scope),
        });
    }

    fn unsupported_binding(&mut self, node: Node<'_>, message: &str) {
        self.ignore_identifier_tree(node);
        self.push_diagnostic(DiagnosticCode::UnsupportedSyntax, message, node);
    }

    fn ignore_identifier_tree(&mut self, node: Node<'_>) {
        if node.kind() == "identifier" {
            self.ignored_identifiers.insert(node.id());
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.ignore_identifier_tree(child);
        }
    }

    fn collect_references(&mut self, node: Node<'_>, scope: ScopeId, function_body: bool) {
        if is_jsx(node.kind()) {
            return;
        }
        let mut scope = self.scope_starts.get(&node.id()).copied().unwrap_or(scope);
        if node.kind() == "statement_block" && function_body {
            // A function body's top-level lexical declarations share its parameter scope.
        } else if let Some(block_scope) = self.scope_starts.get(&node.id()) {
            scope = *block_scope;
        }

        if node.kind() == "identifier"
            && !self.declarations.contains(&node.id())
            && !self.ignored_identifiers.contains(&node.id())
        {
            self.references.push(BindingReference {
                name: node_text(node, self.source).to_owned(),
                span: source_span(self.path, node),
                scope,
                binding: None,
            });
        }

        let body_id = if is_function_like(node.kind()) {
            node.child_by_field_name("body").map(|body| body.id())
        } else {
            None
        };
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_references(child, scope, Some(child.id()) == body_id);
        }
    }

    fn resolve_references(&mut self) {
        let scopes = &self.scopes;
        let names = &self.names_by_scope;
        let bindings = &mut self.references;
        let mut pending_diagnostics = Vec::new();
        for reference in bindings {
            let mut current = Some(reference.scope);
            let mut ambiguous = false;
            while let Some(scope) = current {
                if let Some(candidates) = names
                    .get(&scope)
                    .and_then(|scope_names| scope_names.get(&reference.name))
                {
                    if candidates.len() == 1 {
                        reference.binding = candidates.first().cloned();
                    } else {
                        ambiguous = true;
                        pending_diagnostics.push((
                            DiagnosticCode::AmbiguousMatch,
                            format!(
                                "reference {:?} has multiple declarations in one lexical scope",
                                reference.name
                            ),
                            reference.span.clone(),
                        ));
                    }
                    break;
                }
                current = scopes[scope.0 as usize].parent;
            }
            if reference.binding.is_none() && !ambiguous {
                pending_diagnostics.push((
                    DiagnosticCode::UnresolvedBinding,
                    format!("unresolved lexical reference {:?}", reference.name),
                    reference.span.clone(),
                ));
            }
        }
        for (code, message, span) in pending_diagnostics {
            self.diagnostics.push(Diagnostic {
                code,
                message,
                snapshot: Some(self.snapshot.clone()),
                frontier: None,
                span: Some(span),
                affected_flows: BTreeSet::new(),
            });
        }
    }

    fn new_scope(
        &mut self,
        node: Node<'_>,
        parent: ScopeId,
        kind: ScopeKind,
        enclosing_symbol: Option<String>,
    ) -> ScopeId {
        let id = ScopeId(self.scopes.len() as u64);
        self.scopes.push(LexicalScope {
            id,
            parent: Some(parent),
            kind,
            span: source_span(self.path, node),
            enclosing_symbol,
        });
        self.scope_starts.insert(node.id(), id);
        id
    }

    fn scope_symbol(&self, scope: ScopeId) -> Option<String> {
        self.scopes[scope.0 as usize].enclosing_symbol.clone()
    }

    fn push_diagnostic(&mut self, code: DiagnosticCode, message: &str, node: Node<'_>) {
        self.diagnostics.push(Diagnostic {
            code,
            message: message.into(),
            snapshot: Some(self.snapshot.clone()),
            frontier: None,
            span: Some(source_span(self.path, node)),
            affected_flows: BTreeSet::new(),
        });
    }
}

fn is_function_like(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function_expression"
            | "generator_function_declaration"
            | "generator_function"
            | "arrow_function"
            | "method_definition"
    )
}

fn is_jsx(kind: &str) -> bool {
    kind.starts_with("jsx_")
}

fn function_symbol(node: Node<'_>, source: &str) -> Option<String> {
    if let Some(name) = node.child_by_field_name("name") {
        return Some(node_text(name, source).trim_matches(['\'', '"']).to_owned());
    }
    if node.kind() == "arrow_function"
        && let Some(parent) = node.parent()
        && parent.kind() == "variable_declarator"
        && let Some(name) = parent.child_by_field_name("name")
        && name.kind() == "identifier"
    {
        return Some(node_text(name, source).to_owned());
    }
    None
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

fn source_span(path: &str, node: Node<'_>) -> SourceSpan {
    SourceSpan {
        path: path.into(),
        byte_start: node.start_byte() as u64,
        byte_end: node.end_byte() as u64,
        start_line: node.start_position().row as u32 + 1,
        end_line: inclusive_end_line(node.start_position(), node.end_position()),
    }
}

fn inclusive_end_line(start: Point, end: Point) -> u32 {
    if end.row > start.row && end.column == 0 {
        end.row as u32
    } else {
        end.row as u32 + 1
    }
}

#[cfg(test)]
mod tests;
