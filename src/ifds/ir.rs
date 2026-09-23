//! Language-independent control-flow graph contracts.

use crate::ifds::model::{
    BindingId, CallSiteId, DefinitionId, NodeId, Place, ProcedureId, SourceSpan,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Operation {
    Entry,
    Read {
        source: Place,
        result: Place,
    },
    Write {
        target: Place,
        sources: BTreeSet<Place>,
        definition: DefinitionId,
    },
    Literal {
        definition: DefinitionId,
        literal_kind: LiteralKind,
        raw: String,
        cooked: Option<String>,
        result: Place,
    },
    Compute {
        definition: DefinitionId,
        inputs: Vec<ComputeInput>,
        result: Place,
        operator: PrimitiveOperator,
    },
    Branch {
        condition: Place,
    },
    Join,
    Call {
        call_site: CallSiteId,
        arguments: Vec<Place>,
        result: Option<Place>,
    },
    ReturnSite {
        call_site: CallSiteId,
    },
    Return {
        value: Option<Place>,
    },
    Exit,
    UnknownEffect {
        effect_kind: UnknownEffectKind,
        description: String,
        inputs: Vec<ComputeInput>,
        result: Option<Place>,
        definition: Option<DefinitionId>,
        affected_places: BTreeSet<Place>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiteralKind {
    Number,
    String,
    Boolean,
    Null,
    BigInt,
    Undefined,
    TemplateChunk,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ComputeInputRole {
    Operand { index: u32 },
    TemplateChunk { index: u32 },
    TemplateInterpolation { index: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComputeInput {
    pub place: Place,
    pub role: ComputeInputRole,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PrimitiveOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    UnaryPlus,
    UnaryMinus,
    LogicalNot,
    StrictEqual,
    StrictNotEqual,
    GreaterThan,
    LessThan,
    PropertyRead { property: String },
    IndexRead,
    Template,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownEffectKind {
    Value,
    Control,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrNode {
    pub id: NodeId,
    pub operation: Operation,
    pub span: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EdgeKind {
    Normal,
    Branch { outcome: bool },
    LoopBack,
    Call { call_site: CallSiteId },
    Return { call_site: CallSiteId },
    CallBypass { call_site: CallSiteId },
    Exceptional,
    Deferred,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrEdge {
    pub source: NodeId,
    pub target: NodeId,
    pub kind: EdgeKind,
    pub construct: Option<SourceSpan>,
    pub assumption: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcedureParameter {
    pub index: u32,
    pub binding: BindingId,
    pub entry_definition: DefinitionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcedureIr {
    pub id: ProcedureId,
    pub parameters: Vec<ProcedureParameter>,
    pub entry: NodeId,
    pub exits: BTreeSet<NodeId>,
    pub nodes: BTreeMap<NodeId, IrNode>,
    pub edges: BTreeSet<IrEdge>,
}

impl ProcedureIr {
    pub fn validate(&self) -> Result<(), IrValidationError> {
        let mut parameter_bindings = BTreeSet::new();
        let mut definitions = BTreeSet::new();
        for (expected_index, parameter) in self.parameters.iter().enumerate() {
            if parameter.index as usize != expected_index {
                return Err(IrValidationError::ParameterIndex {
                    expected: expected_index as u32,
                    actual: parameter.index,
                });
            }
            if parameter.binding.snapshot != self.id.snapshot
                || parameter.entry_definition.snapshot != self.id.snapshot
            {
                return Err(IrValidationError::ParameterSnapshotMismatch(
                    parameter.index,
                ));
            }
            if !parameter_bindings.insert(parameter.binding.clone()) {
                return Err(IrValidationError::DuplicateParameterBinding(
                    parameter.binding.clone(),
                ));
            }
            if !definitions.insert(parameter.entry_definition.clone()) {
                return Err(IrValidationError::DuplicateDefinition(
                    parameter.entry_definition.clone(),
                ));
            }
        }
        let entry = self
            .nodes
            .get(&self.entry)
            .ok_or_else(|| IrValidationError::MissingNode(Box::new(self.entry.clone())))?;
        if entry.operation != Operation::Entry {
            return Err(IrValidationError::EntryOperation(Box::new(
                self.entry.clone(),
            )));
        }
        for exit in &self.exits {
            let node = self
                .nodes
                .get(exit)
                .ok_or_else(|| IrValidationError::MissingNode(Box::new(exit.clone())))?;
            if node.operation != Operation::Exit {
                return Err(IrValidationError::ExitOperation(Box::new(exit.clone())));
            }
        }
        for (key, node) in &self.nodes {
            if key != &node.id {
                return Err(IrValidationError::NodeKeyMismatch {
                    key: Box::new(key.clone()),
                    value: Box::new(node.id.clone()),
                });
            }
            if node.id.snapshot != self.id.snapshot {
                return Err(IrValidationError::SnapshotMismatch(Box::new(
                    node.id.clone(),
                )));
            }
            if let Some(span) = &node.span {
                span.validate()
                    .map_err(|error| IrValidationError::InvalidSpan(error.to_string()))?;
            }
        }
        for edge in &self.edges {
            if !self.nodes.contains_key(&edge.source) {
                return Err(IrValidationError::MissingNode(Box::new(
                    edge.source.clone(),
                )));
            }
            if !self.nodes.contains_key(&edge.target) {
                return Err(IrValidationError::MissingNode(Box::new(
                    edge.target.clone(),
                )));
            }
            if let Some(span) = &edge.construct {
                span.validate()
                    .map_err(|error| IrValidationError::InvalidSpan(error.to_string()))?;
            }
        }
        let mut produced = BTreeSet::new();
        for node in self.nodes.values() {
            if let Some(definition) = operation_definition(&node.operation) {
                if definition.snapshot != self.id.snapshot {
                    return Err(IrValidationError::DefinitionSnapshotMismatch(Box::new(
                        definition.clone(),
                    )));
                }
                if !definitions.insert(definition.clone()) {
                    return Err(IrValidationError::DuplicateDefinition(definition.clone()));
                }
            }
            if let Some(result) = operation_result(&node.operation) {
                let Place::Temporary(producer) = result else {
                    return Err(IrValidationError::InvalidResultPlace(Box::new(
                        node.id.clone(),
                    )));
                };
                if producer != &node.id {
                    return Err(IrValidationError::ResultProducerMismatch {
                        node: Box::new(node.id.clone()),
                        producer: Box::new(producer.clone()),
                    });
                }
                if !produced.insert(result.clone()) {
                    return Err(IrValidationError::DuplicateTemporary(result.clone()));
                }
            }
        }
        for node in self.nodes.values() {
            for place in operation_inputs(&node.operation) {
                if matches!(place, Place::Temporary(_)) && !produced.contains(place) {
                    return Err(IrValidationError::MissingTemporary(place.clone()));
                }
            }
        }
        Ok(())
    }
}

fn operation_definition(operation: &Operation) -> Option<&DefinitionId> {
    match operation {
        Operation::Write { definition, .. }
        | Operation::Literal { definition, .. }
        | Operation::Compute { definition, .. } => Some(definition),
        Operation::UnknownEffect { definition, .. } => definition.as_ref(),
        _ => None,
    }
}

fn operation_result(operation: &Operation) -> Option<&Place> {
    match operation {
        Operation::Read { result, .. }
        | Operation::Literal { result, .. }
        | Operation::Compute { result, .. } => Some(result),
        Operation::Call { result, .. } | Operation::UnknownEffect { result, .. } => result.as_ref(),
        _ => None,
    }
}

fn operation_inputs(operation: &Operation) -> Vec<&Place> {
    match operation {
        Operation::Read { source, .. } => vec![source],
        Operation::Write { sources, .. } => sources.iter().collect(),
        Operation::Compute { inputs, .. } | Operation::UnknownEffect { inputs, .. } => {
            inputs.iter().map(|input| &input.place).collect()
        }
        Operation::Branch { condition } => vec![condition],
        Operation::Call { arguments, .. } => arguments.iter().collect(),
        Operation::Return { value } => value.iter().collect(),
        _ => Vec::new(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrValidationError {
    ParameterIndex {
        expected: u32,
        actual: u32,
    },
    ParameterSnapshotMismatch(u32),
    DuplicateParameterBinding(BindingId),
    DuplicateDefinition(DefinitionId),
    DefinitionSnapshotMismatch(Box<DefinitionId>),
    MissingNode(Box<NodeId>),
    EntryOperation(Box<NodeId>),
    ExitOperation(Box<NodeId>),
    NodeKeyMismatch {
        key: Box<NodeId>,
        value: Box<NodeId>,
    },
    SnapshotMismatch(Box<NodeId>),
    InvalidSpan(String),
    InvalidResultPlace(Box<NodeId>),
    ResultProducerMismatch {
        node: Box<NodeId>,
        producer: Box<NodeId>,
    },
    DuplicateTemporary(Place),
    MissingTemporary(Place),
}

impl fmt::Display for IrValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid procedure IR: {self:?}")
    }
}

impl std::error::Error for IrValidationError {}
