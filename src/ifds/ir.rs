//! Language-independent control-flow graph contracts.

use crate::ifds::model::{CallSiteId, DefinitionId, NodeId, Place, ProcedureId, SourceSpan};
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
    Compute {
        inputs: BTreeSet<Place>,
        result: Place,
        operator: String,
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
        description: String,
        affected_places: BTreeSet<Place>,
    },
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcedureIr {
    pub id: ProcedureId,
    pub entry: NodeId,
    pub exits: BTreeSet<NodeId>,
    pub nodes: BTreeMap<NodeId, IrNode>,
    pub edges: BTreeSet<IrEdge>,
}

impl ProcedureIr {
    pub fn validate(&self) -> Result<(), IrValidationError> {
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
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrValidationError {
    MissingNode(Box<NodeId>),
    EntryOperation(Box<NodeId>),
    ExitOperation(Box<NodeId>),
    NodeKeyMismatch {
        key: Box<NodeId>,
        value: Box<NodeId>,
    },
    SnapshotMismatch(Box<NodeId>),
    InvalidSpan(String),
}

impl fmt::Display for IrValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid procedure IR: {self:?}")
    }
}

impl std::error::Error for IrValidationError {}
