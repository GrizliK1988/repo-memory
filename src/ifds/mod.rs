//! Language-independent contracts for variable-flow analysis.
//!
//! This module intentionally contains no parser or language-adapter dependencies.

pub mod adapters;
pub mod fixtures;
pub mod ir;
pub mod model;
pub mod provenance;
pub mod slicing;
pub mod snapshots;
pub mod solver;
pub mod uncertainty;

pub use fixtures::*;
pub use ir::*;
pub use model::*;
pub use provenance::*;
pub use slicing::*;
pub use snapshots::*;
pub use solver::*;
pub use uncertainty::*;

#[cfg(test)]
mod tests;
