//! Language-independent contracts for variable-flow analysis.
//!
//! This module intentionally contains no parser or language-adapter dependencies.

pub mod ir;
pub mod model;
pub mod snapshots;
pub mod solver;

pub use ir::*;
pub use model::*;
pub use snapshots::*;
pub use solver::*;

#[cfg(test)]
mod tests;
