use super::*;
use crate::ifds::ir::{
    ComputeInput, ComputeInputRole, EdgeKind, IrEdge, IrNode, LiteralKind, Operation,
    PrimitiveOperator, ProcedureIr, ProcedureParameter, UnknownEffectKind,
};
use crate::ifds::model::{DefinitionId, NodeId, Place, ProcedureId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeScriptLoweringResult {
    pub procedure: ProcedureIr,
    pub selected_binding: BindingId,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeScriptLoweringError {
    SnapshotMismatch,
    MissingSelectedBinding,
    Parse(String),
    InvalidIr(String),
}

impl fmt::Display for TypeScriptLoweringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypeScript lowering failed: {self:?}")
    }
}

impl std::error::Error for TypeScriptLoweringError {}

pub fn lower_containing_procedure(
    source: &str,
    index: &TypeScriptBindingIndex,
    selected_binding: &BindingId,
) -> Result<TypeScriptLoweringResult, TypeScriptLoweringError> {
    if selected_binding.snapshot != index.snapshot {
        return Err(TypeScriptLoweringError::SnapshotMismatch);
    }
    let selected = index
        .bindings
        .iter()
        .find(|binding| &binding.id == selected_binding)
        .ok_or(TypeScriptLoweringError::MissingSelectedBinding)?;

    let mut parser = Parser::new();
    let language = match index.grammar {
        TypeScriptGrammar::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        TypeScriptGrammar::Tsx => tree_sitter_typescript::LANGUAGE_TSX,
    };
    parser
        .set_language(&language.into())
        .map_err(|error| TypeScriptLoweringError::Parse(error.to_string()))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| TypeScriptLoweringError::Parse("parser returned no tree".into()))?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(TypeScriptLoweringError::Parse(
            "source contains a syntax error".into(),
        ));
    }

    let declaration = find_exact_node(
        root,
        selected.declaration.byte_start as usize,
        selected.declaration.byte_end as usize,
        "identifier",
    )
    .ok_or(TypeScriptLoweringError::MissingSelectedBinding)?;
    let container = containing_procedure(declaration, root);
    let ordinal = procedure_ordinal(root, container);
    let procedure_scope = procedure_scope(index, selected.scope);
    let mut lowerer = Lowerer::new(source, index, ordinal, procedure_scope);
    lowerer.lower(container);
    let result = lowerer.finish(selected_binding.clone())?;
    Ok(result)
}

struct Lowerer<'a> {
    source: &'a str,
    index: &'a TypeScriptBindingIndex,
    procedure_ordinal: u64,
    procedure_scope: ScopeId,
    next_operation: u64,
    entry: NodeId,
    tail: Option<NodeId>,
    terminals: Vec<NodeId>,
    nodes: BTreeMap<NodeId, IrNode>,
    edges: BTreeSet<IrEdge>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Lowerer<'a> {
    fn new(
        source: &'a str,
        index: &'a TypeScriptBindingIndex,
        procedure_ordinal: u64,
        procedure_scope: ScopeId,
    ) -> Self {
        let entry = NodeId::new(index.snapshot.clone(), procedure_ordinal << 32 | 1);
        let mut nodes = BTreeMap::new();
        nodes.insert(
            entry.clone(),
            IrNode {
                id: entry.clone(),
                operation: Operation::Entry,
                span: None,
            },
        );
        Self {
            source,
            index,
            procedure_ordinal,
            procedure_scope,
            next_operation: 2,
            entry: entry.clone(),
            tail: Some(entry),
            terminals: Vec::new(),
            nodes,
            edges: BTreeSet::new(),
            diagnostics: Vec::new(),
        }
    }

    fn lower(&mut self, container: Node<'_>) {
        if container.kind() == "program" {
            self.lower_children(container);
            return;
        }
        if let Some(body) = container.child_by_field_name("body") {
            if body.kind() == "statement_block" {
                self.lower_children(body);
            } else {
                let value = self.lower_expression(body);
                let ret = self.emit(
                    Operation::Return { value: Some(value) },
                    Some(source_span(&self.index.path, body)),
                );
                self.terminals.push(ret);
                self.tail = None;
            }
        }
    }

    fn lower_children(&mut self, parent: Node<'_>) {
        let mut cursor = parent.walk();
        for child in parent.named_children(&mut cursor) {
            self.lower_statement(child);
        }
    }

    fn lower_statement(&mut self, node: Node<'_>) {
        match node.kind() {
            "lexical_declaration" => self.lower_declaration(node),
            "expression_statement" => {
                if let Some(expression) = node.named_child(0) {
                    if expression.kind() == "assignment_expression" {
                        self.lower_assignment(expression);
                    } else {
                        self.lower_expression(expression);
                    }
                }
            }
            "return_statement" => self.lower_return(node),
            "if_statement" => self.lower_if(node),
            "function_declaration" | "generator_function_declaration" => {
                if self.contains_procedure_capture(node) {
                    self.emit_unknown(node, UnknownEffectKind::Value, Vec::new(), false);
                }
            }
            "export_statement" => {
                let mut cursor = node.walk();
                let mut lowered_declaration = false;
                for child in node.named_children(&mut cursor) {
                    match child.kind() {
                        "lexical_declaration"
                        | "function_declaration"
                        | "generator_function_declaration" => {
                            self.lower_statement(child);
                            lowered_declaration = true;
                        }
                        _ => {}
                    }
                }
                if !lowered_declaration {
                    let inputs = identifier_nodes(node)
                        .into_iter()
                        .filter_map(|identifier| {
                            self.reference_binding_at(identifier).map(|_| {
                                let place = self.lower_expression(identifier);
                                input(
                                    place,
                                    ComputeInputRole::Operand { index: 0 },
                                    self.index,
                                    identifier,
                                )
                            })
                        })
                        .enumerate()
                        .map(|(index, mut input)| {
                            input.role = ComputeInputRole::Operand {
                                index: index as u32,
                            };
                            input
                        })
                        .collect();
                    self.emit_unknown(node, UnknownEffectKind::Value, inputs, false);
                }
            }
            "import_statement" => {
                self.emit_unknown(node, UnknownEffectKind::Value, Vec::new(), false);
            }
            "empty_statement" | "comment" => {}
            "statement_block" => self.lower_children(node),
            _ => {
                self.emit_unknown(node, UnknownEffectKind::Control, Vec::new(), false);
                self.tail = None;
            }
        }
    }

    fn lower_declaration(&mut self, declaration: Node<'_>) {
        let mut cursor = declaration.walk();
        for declarator in declaration.named_children(&mut cursor) {
            if declarator.kind() != "variable_declarator" {
                continue;
            }
            let Some(name) = declarator.child_by_field_name("name") else {
                self.emit_unknown(declarator, UnknownEffectKind::Value, Vec::new(), false);
                continue;
            };
            if name.kind() != "identifier" {
                self.emit_unknown(declarator, UnknownEffectKind::Value, Vec::new(), false);
                continue;
            }
            let Some(binding) = self.binding_at(name) else {
                self.emit_unknown(declarator, UnknownEffectKind::Value, Vec::new(), false);
                continue;
            };
            let value = declarator
                .child_by_field_name("value")
                .map(|value| self.lower_expression(value))
                .unwrap_or_else(|| {
                    self.emit_literal(LiteralKind::Undefined, "undefined", None, name)
                });
            let node_id = self.peek_id();
            let definition = DefinitionId::new(self.index.snapshot.clone(), node_id.local);
            self.emit(
                Operation::Write {
                    target: Place::Binding(binding),
                    sources: BTreeSet::from([value]),
                    definition,
                },
                Some(source_span(&self.index.path, declarator)),
            );
        }
    }

    fn lower_assignment(&mut self, assignment: Node<'_>) {
        let left = assignment.child_by_field_name("left");
        let right = assignment.child_by_field_name("right");
        let Some(left) = left else {
            self.emit_unknown(assignment, UnknownEffectKind::Value, Vec::new(), false);
            return;
        };
        let Some(right) = right else {
            self.emit_unknown(assignment, UnknownEffectKind::Value, Vec::new(), true);
            return;
        };
        let operator = operator_between(left, right, self.source).trim();
        if left.kind() != "identifier" || operator != "=" {
            self.emit_unknown(assignment, UnknownEffectKind::Value, Vec::new(), true);
            return;
        }
        let Some(target) = self.reference_binding_at(left) else {
            self.emit_unknown(assignment, UnknownEffectKind::Value, Vec::new(), true);
            return;
        };
        let value = self.lower_expression(right);
        let node_id = self.peek_id();
        self.emit(
            Operation::Write {
                target: Place::Binding(target),
                sources: BTreeSet::from([value]),
                definition: DefinitionId::new(self.index.snapshot.clone(), node_id.local),
            },
            Some(source_span(&self.index.path, assignment)),
        );
    }

    fn lower_return(&mut self, node: Node<'_>) {
        let value = node
            .named_child(0)
            .map(|value| self.lower_expression(value));
        let id = self.emit(
            Operation::Return { value },
            Some(source_span(&self.index.path, node)),
        );
        self.terminals.push(id);
        self.tail = None;
    }

    fn lower_if(&mut self, node: Node<'_>) {
        let Some(condition_node) = node.child_by_field_name("condition") else {
            self.emit_unknown(node, UnknownEffectKind::Control, Vec::new(), false);
            self.tail = None;
            return;
        };
        let Some(consequence) = node.child_by_field_name("consequence") else {
            self.emit_unknown(node, UnknownEffectKind::Control, Vec::new(), false);
            self.tail = None;
            return;
        };
        let condition = self.lower_expression(condition_node);
        let branch = self.emit(
            Operation::Branch { condition },
            Some(source_span(&self.index.path, condition_node)),
        );
        let construct = source_span(&self.index.path, node);
        let label = node_text(condition_node, self.source).trim().to_owned();
        let alternative = node
            .child_by_field_name("alternative")
            .and_then(|alternative| {
                if alternative.kind() == "else_clause" {
                    alternative.named_child(0)
                } else {
                    Some(alternative)
                }
            });
        let mut continuing = Vec::new();
        for (outcome, arm) in [(true, Some(consequence)), (false, alternative)] {
            self.tail = None;
            let entry = self.emit(Operation::Join, None);
            self.edges.insert(IrEdge {
                source: branch.clone(),
                target: entry,
                kind: EdgeKind::Branch { outcome },
                construct: Some(construct.clone()),
                assumption: Some(if outcome {
                    label.clone()
                } else {
                    format!("!({label})")
                }),
            });
            if let Some(arm) = arm {
                self.lower_statement(arm);
            }
            if let Some(tail) = self.tail.take() {
                continuing.push(tail);
            }
        }
        if continuing.is_empty() {
            self.tail = None;
        } else {
            self.tail = None;
            let join = self.emit(Operation::Join, None);
            for tail in continuing {
                self.edges.insert(normal_edge(tail, join.clone()));
            }
        }
    }

    fn lower_expression(&mut self, node: Node<'_>) -> Place {
        match node.kind() {
            "identifier" => {
                if node_text(node, self.source) == "undefined"
                    && self.reference_binding_at(node).is_none()
                {
                    return self.emit_literal(LiteralKind::Undefined, "undefined", None, node);
                }
                if let Some(binding) = self.reference_binding_at(node) {
                    let id = self.peek_id();
                    let result = Place::Temporary(id.clone());
                    self.emit(
                        Operation::Read {
                            source: Place::Binding(binding),
                            result: result.clone(),
                        },
                        Some(source_span(&self.index.path, node)),
                    );
                    return result;
                }
                self.emit_unknown(node, UnknownEffectKind::Value, Vec::new(), true)
                    .expect("value effect has a result")
            }
            "number" => {
                let raw = node_text(node, self.source);
                let kind = if raw.ends_with('n') {
                    LiteralKind::BigInt
                } else {
                    LiteralKind::Number
                };
                self.emit_literal(kind, raw, None, node)
            }
            "string" => self.emit_literal(
                LiteralKind::String,
                node_text(node, self.source),
                cooked_string(node_text(node, self.source)),
                node,
            ),
            "true" | "false" => self.emit_literal(
                LiteralKind::Boolean,
                node_text(node, self.source),
                Some(node_text(node, self.source).into()),
                node,
            ),
            "null" => self.emit_literal(LiteralKind::Null, "null", Some("null".into()), node),
            "parenthesized_expression"
            | "as_expression"
            | "satisfies_expression"
            | "non_null_expression"
            | "type_assertion" => {
                let inner = node
                    .child_by_field_name("expression")
                    .or_else(|| node.named_child(0));
                inner
                    .map(|inner| self.lower_expression(inner))
                    .unwrap_or_else(|| {
                        self.emit_unknown(node, UnknownEffectKind::Value, Vec::new(), true)
                            .unwrap()
                    })
            }
            "binary_expression" => self.lower_binary(node),
            "unary_expression" => self.lower_unary(node),
            "member_expression" => self.lower_member(node),
            "subscript_expression" => self.lower_subscript(node),
            "template_string" => self.lower_template(node),
            "call_expression" | "new_expression" => self.lower_call_like(node),
            _ => self
                .emit_unknown(node, UnknownEffectKind::Value, Vec::new(), true)
                .unwrap(),
        }
    }

    fn lower_binary(&mut self, node: Node<'_>) -> Place {
        let Some(left) = node.child_by_field_name("left") else {
            return self
                .emit_unknown(node, UnknownEffectKind::Value, Vec::new(), true)
                .unwrap();
        };
        let Some(right) = node.child_by_field_name("right") else {
            return self
                .emit_unknown(node, UnknownEffectKind::Value, Vec::new(), true)
                .unwrap();
        };
        let operator_text = node
            .child_by_field_name("operator")
            .map(|operator| node_text(operator, self.source))
            .unwrap_or_else(|| operator_between(left, right, self.source));
        let Some(operator) = primitive_binary(operator_text) else {
            return self
                .emit_unknown(node, UnknownEffectKind::Value, Vec::new(), true)
                .unwrap();
        };
        let left_place = self.lower_expression(left);
        let right_place = self.lower_expression(right);
        self.emit_compute(
            node,
            operator,
            vec![
                input(
                    left_place,
                    ComputeInputRole::Operand { index: 0 },
                    self.index,
                    left,
                ),
                input(
                    right_place,
                    ComputeInputRole::Operand { index: 1 },
                    self.index,
                    right,
                ),
            ],
        )
    }

    fn lower_unary(&mut self, node: Node<'_>) -> Place {
        let Some(argument) = node
            .child_by_field_name("argument")
            .or_else(|| node.named_child(0))
        else {
            return self
                .emit_unknown(node, UnknownEffectKind::Value, Vec::new(), true)
                .unwrap();
        };
        let prefix = &self.source[node.start_byte()..argument.start_byte()];
        let operator = match prefix.trim() {
            "+" => PrimitiveOperator::UnaryPlus,
            "-" => PrimitiveOperator::UnaryMinus,
            "!" => PrimitiveOperator::LogicalNot,
            _ => {
                return self
                    .emit_unknown(node, UnknownEffectKind::Value, Vec::new(), true)
                    .unwrap();
            }
        };
        let place = self.lower_expression(argument);
        self.emit_compute(
            node,
            operator,
            vec![input(
                place,
                ComputeInputRole::Operand { index: 0 },
                self.index,
                argument,
            )],
        )
    }

    fn lower_member(&mut self, node: Node<'_>) -> Place {
        let Some(object) = node.child_by_field_name("object") else {
            return self
                .emit_unknown(node, UnknownEffectKind::Value, Vec::new(), true)
                .unwrap();
        };
        let property = node
            .child_by_field_name("property")
            .map(|property| node_text(property, self.source).to_owned())
            .unwrap_or_default();
        let place = self.lower_expression(object);
        self.emit_compute(
            node,
            PrimitiveOperator::PropertyRead { property },
            vec![input(
                place,
                ComputeInputRole::Operand { index: 0 },
                self.index,
                object,
            )],
        )
    }

    fn lower_subscript(&mut self, node: Node<'_>) -> Place {
        let Some(object) = node.child_by_field_name("object") else {
            return self
                .emit_unknown(node, UnknownEffectKind::Value, Vec::new(), true)
                .unwrap();
        };
        let Some(index_node) = node.child_by_field_name("index") else {
            return self
                .emit_unknown(node, UnknownEffectKind::Value, Vec::new(), true)
                .unwrap();
        };
        let object_place = self.lower_expression(object);
        let index_place = self.lower_expression(index_node);
        self.emit_compute(
            node,
            PrimitiveOperator::IndexRead,
            vec![
                input(
                    object_place,
                    ComputeInputRole::Operand { index: 0 },
                    self.index,
                    object,
                ),
                input(
                    index_place,
                    ComputeInputRole::Operand { index: 1 },
                    self.index,
                    index_node,
                ),
            ],
        )
    }

    fn lower_template(&mut self, node: Node<'_>) -> Place {
        let mut inputs = Vec::new();
        let mut cursor = node.walk();
        let mut chunk_index = 0;
        let mut interpolation_index = 0;
        for child in node.named_children(&mut cursor) {
            if child.kind() == "template_substitution" {
                if let Some(expression) = child.named_child(0) {
                    let place = self.lower_expression(expression);
                    inputs.push(input(
                        place,
                        ComputeInputRole::TemplateInterpolation {
                            index: interpolation_index,
                        },
                        self.index,
                        expression,
                    ));
                    interpolation_index += 1;
                }
            } else {
                let raw = node_text(child, self.source);
                let cooked = (!raw.contains('\\')).then(|| raw.into());
                let place = self.emit_literal(LiteralKind::TemplateChunk, raw, cooked, child);
                inputs.push(input(
                    place,
                    ComputeInputRole::TemplateChunk { index: chunk_index },
                    self.index,
                    child,
                ));
                chunk_index += 1;
            }
        }
        self.emit_compute(node, PrimitiveOperator::Template, inputs)
    }

    fn lower_call_like(&mut self, node: Node<'_>) -> Place {
        let mut inputs = Vec::new();
        if let Some(function) = node
            .child_by_field_name("function")
            .or_else(|| node.child_by_field_name("constructor"))
        {
            let place = self.lower_expression(function);
            inputs.push(input(
                place,
                ComputeInputRole::Operand { index: 0 },
                self.index,
                function,
            ));
        }
        if let Some(arguments) = node.child_by_field_name("arguments") {
            let mut cursor = arguments.walk();
            for (offset, argument) in arguments.named_children(&mut cursor).enumerate() {
                let place = self.lower_expression(argument);
                inputs.push(input(
                    place,
                    ComputeInputRole::Operand {
                        index: offset as u32 + 1,
                    },
                    self.index,
                    argument,
                ));
            }
        }
        self.emit_unknown(node, UnknownEffectKind::Value, inputs, true)
            .unwrap()
    }

    fn emit_literal(
        &mut self,
        kind: LiteralKind,
        raw: &str,
        cooked: Option<String>,
        node: Node<'_>,
    ) -> Place {
        let id = self.peek_id();
        let result = Place::Temporary(id.clone());
        self.emit(
            Operation::Literal {
                definition: DefinitionId::new(self.index.snapshot.clone(), id.local),
                literal_kind: kind,
                raw: raw.into(),
                cooked,
                result: result.clone(),
            },
            Some(source_span(&self.index.path, node)),
        );
        result
    }

    fn emit_compute(
        &mut self,
        node: Node<'_>,
        operator: PrimitiveOperator,
        inputs: Vec<ComputeInput>,
    ) -> Place {
        let id = self.peek_id();
        let result = Place::Temporary(id.clone());
        self.emit(
            Operation::Compute {
                definition: DefinitionId::new(self.index.snapshot.clone(), id.local),
                inputs,
                result: result.clone(),
                operator,
            },
            Some(source_span(&self.index.path, node)),
        );
        result
    }

    fn emit_unknown(
        &mut self,
        node: Node<'_>,
        effect_kind: UnknownEffectKind,
        inputs: Vec<ComputeInput>,
        has_result: bool,
    ) -> Option<Place> {
        let id = self.peek_id();
        let result = has_result.then(|| Place::Temporary(id.clone()));
        let definition =
            has_result.then(|| DefinitionId::new(self.index.snapshot.clone(), id.local));
        let span = source_span(&self.index.path, node);
        let description = format!("unsupported TypeScript construct {:?}", node.kind());
        let emitted = self.emit(
            Operation::UnknownEffect {
                effect_kind,
                description: description.clone(),
                inputs,
                result: result.clone(),
                definition,
                affected_places: self.affected_places(node.start_byte()),
            },
            Some(span.clone()),
        );
        self.diagnostics.push(Diagnostic {
            code: DiagnosticCode::UnsupportedSyntax,
            message: description,
            snapshot: Some(self.index.snapshot.clone()),
            frontier: Some(emitted),
            span: Some(span),
            affected_flows: BTreeSet::new(),
        });
        result
    }

    fn affected_places(&self, byte: usize) -> BTreeSet<Place> {
        self.index
            .bindings
            .iter()
            .filter(|binding| {
                binding.kind != BindingKind::Const
                    && binding.declaration.byte_start <= byte as u64
                    && scope_belongs_to_procedure(self.index, binding.scope, self.procedure_scope)
            })
            .map(|binding| Place::Binding(binding.id.clone()))
            .collect()
    }

    fn binding_at(&self, node: Node<'_>) -> Option<BindingId> {
        self.index
            .bindings
            .iter()
            .find(|binding| {
                binding.declaration.byte_start == node.start_byte() as u64
                    && binding.declaration.byte_end == node.end_byte() as u64
            })
            .map(|binding| binding.id.clone())
    }

    fn reference_binding_at(&self, node: Node<'_>) -> Option<BindingId> {
        self.index
            .references
            .iter()
            .find(|reference| {
                reference.span.byte_start == node.start_byte() as u64
                    && reference.span.byte_end == node.end_byte() as u64
            })
            .and_then(|reference| reference.binding.clone())
    }

    fn contains_procedure_capture(&self, node: Node<'_>) -> bool {
        self.index.references.iter().any(|reference| {
            reference.span.byte_start >= node.start_byte() as u64
                && reference.span.byte_end <= node.end_byte() as u64
                && reference.binding.as_ref().is_some_and(|binding_id| {
                    self.index.bindings.iter().any(|binding| {
                        &binding.id == binding_id
                            && scope_belongs_to_procedure(
                                self.index,
                                binding.scope,
                                self.procedure_scope,
                            )
                    })
                })
        })
    }

    fn peek_id(&self) -> NodeId {
        NodeId::new(
            self.index.snapshot.clone(),
            self.procedure_ordinal << 32 | self.next_operation,
        )
    }

    fn emit(&mut self, operation: Operation, span: Option<SourceSpan>) -> NodeId {
        let id = self.peek_id();
        self.next_operation += 1;
        self.nodes.insert(
            id.clone(),
            IrNode {
                id: id.clone(),
                operation,
                span,
            },
        );
        if let Some(previous) = self.tail.take() {
            self.edges.insert(normal_edge(previous, id.clone()));
        }
        self.tail = Some(id.clone());
        id
    }

    fn finish(
        mut self,
        selected_binding: BindingId,
    ) -> Result<TypeScriptLoweringResult, TypeScriptLoweringError> {
        let exit = self.peek_id();
        self.nodes.insert(
            exit.clone(),
            IrNode {
                id: exit.clone(),
                operation: Operation::Exit,
                span: None,
            },
        );
        if let Some(tail) = self.tail.take() {
            self.edges.insert(normal_edge(tail, exit.clone()));
        }
        for terminal in self.terminals {
            self.edges.insert(normal_edge(terminal, exit.clone()));
        }
        let procedure = ProcedureIr {
            id: ProcedureId::new(self.index.snapshot.clone(), self.procedure_ordinal),
            parameters: procedure_parameters(
                self.index,
                self.procedure_scope,
                self.procedure_ordinal,
            ),
            entry: self.entry,
            exits: BTreeSet::from([exit]),
            nodes: self.nodes,
            edges: self.edges,
        };
        procedure
            .validate()
            .map_err(|error| TypeScriptLoweringError::InvalidIr(error.to_string()))?;
        self.diagnostics.sort();
        Ok(TypeScriptLoweringResult {
            procedure,
            selected_binding,
            diagnostics: self.diagnostics,
        })
    }
}

fn procedure_parameters(
    index: &TypeScriptBindingIndex,
    procedure_scope: ScopeId,
    procedure_ordinal: u64,
) -> Vec<ProcedureParameter> {
    index
        .bindings
        .iter()
        .filter(|binding| {
            binding.scope == procedure_scope && binding.kind == BindingKind::Parameter
        })
        .enumerate()
        .map(|(position, binding)| ProcedureParameter {
            index: position as u32,
            binding: binding.id.clone(),
            entry_definition: DefinitionId::new(
                index.snapshot.clone(),
                (procedure_ordinal << 32) | (1_u64 << 31) | position as u64,
            ),
        })
        .collect()
}

fn normal_edge(source: NodeId, target: NodeId) -> IrEdge {
    IrEdge {
        source,
        target,
        kind: EdgeKind::Normal,
        construct: None,
        assumption: None,
    }
}

fn input(
    place: Place,
    role: ComputeInputRole,
    index: &TypeScriptBindingIndex,
    node: Node<'_>,
) -> ComputeInput {
    ComputeInput {
        place,
        role,
        span: source_span(&index.path, node),
    }
}

fn primitive_binary(operator: &str) -> Option<PrimitiveOperator> {
    Some(match operator.trim() {
        "+" => PrimitiveOperator::Add,
        "-" => PrimitiveOperator::Subtract,
        "*" => PrimitiveOperator::Multiply,
        "/" => PrimitiveOperator::Divide,
        "%" => PrimitiveOperator::Remainder,
        "===" => PrimitiveOperator::StrictEqual,
        "!==" => PrimitiveOperator::StrictNotEqual,
        ">" => PrimitiveOperator::GreaterThan,
        "<" => PrimitiveOperator::LessThan,
        _ => return None,
    })
}

fn operator_between<'a>(left: Node<'_>, right: Node<'_>, source: &'a str) -> &'a str {
    &source[left.end_byte()..right.start_byte()]
}

fn cooked_string(raw: &str) -> Option<String> {
    serde_json::from_str(raw).ok()
}

fn find_exact_node<'tree>(
    node: Node<'tree>,
    start: usize,
    end: usize,
    kind: &str,
) -> Option<Node<'tree>> {
    if node.start_byte() == start && node.end_byte() == end && node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find_map(|child| find_exact_node(child, start, end, kind))
}

fn identifier_nodes(node: Node<'_>) -> Vec<Node<'_>> {
    fn collect<'tree>(node: Node<'tree>, found: &mut Vec<Node<'tree>>) {
        if node.kind() == "identifier" {
            found.push(node);
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            collect(child, found);
        }
    }
    let mut found = Vec::new();
    collect(node, &mut found);
    found
}

fn containing_procedure<'tree>(mut node: Node<'tree>, root: Node<'tree>) -> Node<'tree> {
    while let Some(parent) = node.parent() {
        if is_function_like(parent.kind()) {
            return parent;
        }
        node = parent;
    }
    root
}

fn procedure_ordinal(root: Node<'_>, target: Node<'_>) -> u64 {
    if target.kind() == "program" {
        return 1;
    }
    fn walk(node: Node<'_>, target_id: usize, ordinal: &mut u64) -> Option<u64> {
        if is_function_like(node.kind()) {
            *ordinal += 1;
            if node.id() == target_id {
                return Some(*ordinal);
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if let Some(found) = walk(child, target_id, ordinal) {
                return Some(found);
            }
        }
        None
    }
    let mut ordinal = 1;
    walk(root, target.id(), &mut ordinal).unwrap_or(1)
}

fn procedure_scope(index: &TypeScriptBindingIndex, mut scope: ScopeId) -> ScopeId {
    loop {
        let current = &index.scopes[scope.0 as usize];
        if matches!(current.kind, ScopeKind::Function | ScopeKind::Program) {
            return scope;
        }
        scope = current.parent.expect("block scope has a parent");
    }
}

fn scope_belongs_to_procedure(
    index: &TypeScriptBindingIndex,
    mut scope: ScopeId,
    procedure: ScopeId,
) -> bool {
    loop {
        if scope == procedure {
            return true;
        }
        let current = &index.scopes[scope.0 as usize];
        if current.kind == ScopeKind::Function {
            return false;
        }
        let Some(parent) = current.parent else {
            return false;
        };
        scope = parent;
    }
}
