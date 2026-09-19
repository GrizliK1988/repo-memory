//! Generic solver boundary; concrete solving begins in later kickoff tasks.

use crate::ifds::ir::ProcedureIr;
use crate::ifds::model::{AnalysisLimits, AnalysisStats, Diagnostic, Fact, NodeId};
use crate::ifds::provenance::TransferOutcome;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolverRequest<'a> {
    pub procedure: &'a ProcedureIr,
    pub entry_facts: BTreeSet<Fact>,
    pub limits: AnalysisLimits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolverOutput {
    pub facts_at: BTreeMap<NodeId, BTreeSet<Fact>>,
    pub diagnostics: BTreeSet<Diagnostic>,
    pub stats: AnalysisStats,
}

pub trait Solver {
    fn solve(&self, request: SolverRequest<'_>) -> Result<SolverOutput, SolverError>;
}

pub trait FlowFunction {
    fn transfer(&self, node: &NodeId, fact: &Fact) -> TransferOutcome;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolverError {
    InvalidIr(String),
    Internal(String),
}

impl fmt::Display for SolverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIr(reason) => write!(f, "invalid solver IR: {reason}"),
            Self::Internal(reason) => write!(f, "solver failed: {reason}"),
        }
    }
}

impl std::error::Error for SolverError {}
