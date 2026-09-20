//! Explicit uncertainty, regional coverage, and bounded report output.

use crate::ifds::model::{
    AnalysisLimitKind, AnalysisRegion, Coverage, Diagnostic, DiagnosticCode, Direction,
    EvidenceKind, LimitReason, NodeId, RelationPresence, SourceSpan, UnknownFrontier,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UncertaintyTracker {
    coverage: BTreeMap<AnalysisRegion, Coverage>,
    diagnostics: BTreeSet<Diagnostic>,
    frontiers: BTreeSet<UnknownFrontier>,
}

impl UncertaintyTracker {
    pub fn register(&mut self, region: AnalysisRegion, coverage: Coverage) {
        self.coverage.entry(region).or_insert(coverage);
    }

    pub fn mark_unknown(
        &mut self,
        region: AnalysisRegion,
        diagnostic: Diagnostic,
        frontier: UnknownFrontier,
    ) {
        self.coverage
            .entry(region)
            .and_modify(|coverage| {
                if *coverage == Coverage::Complete {
                    *coverage = Coverage::Partial;
                }
            })
            .or_insert(Coverage::Partial);
        self.diagnostics.insert(diagnostic);
        self.frontiers.insert(frontier);
    }

    pub fn coverage(&self, region: &AnalysisRegion) -> Option<Coverage> {
        self.coverage.get(region).copied()
    }

    pub fn region_completeness(&self) -> &BTreeMap<AnalysisRegion, Coverage> {
        &self.coverage
    }

    pub fn diagnostics(&self) -> &BTreeSet<Diagnostic> {
        &self.diagnostics
    }

    pub fn frontiers(&self) -> &BTreeSet<UnknownFrontier> {
        &self.frontiers
    }
}

/// Classifies one relation without spreading a partial result to unrelated
/// relations. Positive supported/modelled evidence remains present even if some
/// other continuation is unresolved.
pub fn relation_presence<'a>(
    evidence: impl IntoIterator<Item = &'a EvidenceKind>,
    coverage: Coverage,
) -> RelationPresence {
    let mut unresolved = false;
    for item in evidence {
        match item {
            EvidenceKind::Supported | EvidenceKind::Modeled { .. } => {
                return RelationPresence::Present;
            }
            EvidenceKind::Unresolved { .. } => unresolved = true,
        }
    }
    if unresolved || coverage != Coverage::Complete {
        RelationPresence::Unknown
    } else {
        RelationPresence::Absent
    }
}

pub fn localized_unknown_boundary(
    code: DiagnosticCode,
    message: impl Into<String>,
    node: NodeId,
    span: Option<SourceSpan>,
    direction: Direction,
    next_operation: impl Into<String>,
) -> (Diagnostic, UnknownFrontier) {
    let affected = match direction {
        Direction::Upstream => "upstream",
        Direction::Downstream => "downstream",
    };
    (
        Diagnostic {
            code: code.clone(),
            message: message.into(),
            snapshot: Some(node.snapshot.clone()),
            frontier: Some(node.clone()),
            span,
            affected_flows: BTreeSet::from([affected.into()]),
        },
        UnknownFrontier {
            node,
            next_operation: next_operation.into(),
            diagnostic: code,
            affected_direction: direction,
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedOutput<T> {
    pub items: Vec<T>,
    pub omitted: u64,
    pub limit_reason: Option<LimitReason>,
    /// Whether truncation affects solved graph completeness. Witness-only
    /// display truncation leaves this true.
    pub analysis_complete: bool,
}

pub fn limit_witnesses<T>(items: impl IntoIterator<Item = T>, limit: u32) -> BoundedOutput<T> {
    limit_items(
        items,
        u64::from(limit),
        AnalysisLimitKind::WitnessesPerRelation,
        false,
    )
}

pub fn limit_graph_nodes<T>(items: impl IntoIterator<Item = T>, limit: u64) -> BoundedOutput<T> {
    limit_items(items, limit, AnalysisLimitKind::OutputNodes, true)
}

fn limit_items<T>(
    items: impl IntoIterator<Item = T>,
    limit: u64,
    kind: AnalysisLimitKind,
    truncation_is_incomplete: bool,
) -> BoundedOutput<T> {
    let mut retained = Vec::new();
    let mut omitted = 0_u64;
    for item in items {
        if (retained.len() as u64) < limit {
            retained.push(item);
        } else {
            omitted = omitted.saturating_add(1);
        }
    }
    let was_truncated = omitted > 0;
    BoundedOutput {
        items: retained,
        omitted,
        limit_reason: was_truncated.then_some(LimitReason {
            kind,
            limit,
            observed: limit.saturating_add(omitted),
        }),
        analysis_complete: !(was_truncated && truncation_is_incomplete),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ifds::model::{SnapshotId, SnapshotSide};

    fn snapshot() -> SnapshotId {
        SnapshotId {
            side: SnapshotSide::Before,
            revision: "k009".into(),
            content_id: "k009-content".into(),
        }
    }

    fn node(local: u64) -> NodeId {
        NodeId::new(snapshot(), local)
    }

    fn span(line: u32) -> SourceSpan {
        SourceSpan {
            path: "src/main.ts".into(),
            byte_start: u64::from(line - 1) * 10,
            byte_end: u64::from(line) * 10,
            start_line: line,
            end_line: line,
        }
    }

    #[test]
    fn ifds_k009_unaffected_region_survives() {
        let affected = AnalysisRegion {
            name: "selected-result".into(),
            direction: Direction::Downstream,
        };
        let independent = AnalysisRegion {
            name: "independent-input".into(),
            direction: Direction::Upstream,
        };
        let (diagnostic, frontier) = localized_unknown_boundary(
            DiagnosticCode::UnresolvedCall,
            "unknown call",
            node(2),
            Some(span(2)),
            Direction::Downstream,
            "call",
        );
        let mut tracker = UncertaintyTracker::default();
        tracker.register(affected.clone(), Coverage::Complete);
        tracker.register(independent.clone(), Coverage::Complete);
        tracker.mark_unknown(affected.clone(), diagnostic, frontier);

        assert_eq!(tracker.coverage(&affected), Some(Coverage::Partial));
        assert_eq!(tracker.coverage(&independent), Some(Coverage::Complete));
        assert_eq!(
            relation_presence([&EvidenceKind::Supported], Coverage::Complete),
            RelationPresence::Present
        );
    }

    #[test]
    fn ifds_k009_no_absence_from_empty() {
        assert_eq!(
            relation_presence(std::iter::empty(), Coverage::Partial),
            RelationPresence::Unknown
        );
        assert_eq!(
            relation_presence(std::iter::empty(), Coverage::Unsupported),
            RelationPresence::Unknown
        );
        assert_eq!(
            relation_presence(std::iter::empty(), Coverage::Complete),
            RelationPresence::Absent
        );
    }

    #[test]
    fn ifds_k009_witness_vs_graph_limit() {
        let solved_relations = BTreeSet::from(["write->read", "read->return"]);
        let witnesses = limit_witnesses(["w1", "w2", "w3"], 1);
        assert_eq!(witnesses.items, vec!["w1"]);
        assert_eq!(witnesses.omitted, 2);
        assert!(witnesses.analysis_complete);
        assert_eq!(solved_relations.len(), 2);
        assert_eq!(
            witnesses.limit_reason.unwrap().kind,
            AnalysisLimitKind::WitnessesPerRelation
        );

        let graph = limit_graph_nodes([node(1), node(2), node(3)], 2);
        assert_eq!(graph.items, vec![node(1), node(2)]);
        assert_eq!(graph.omitted, 1);
        assert!(!graph.analysis_complete);
        assert_eq!(
            graph.limit_reason.unwrap(),
            LimitReason {
                kind: AnalysisLimitKind::OutputNodes,
                limit: 2,
                observed: 3,
            }
        );
        assert_eq!(
            relation_presence(std::iter::empty(), Coverage::Partial),
            RelationPresence::Unknown
        );
    }

    #[test]
    fn ifds_k009_diagnostic_location() {
        let cases = [
            (DiagnosticCode::ParseError, Direction::Upstream),
            (DiagnosticCode::MissingDependency, Direction::Upstream),
            (DiagnosticCode::UnsupportedSyntax, Direction::Downstream),
            (DiagnosticCode::UnresolvedCall, Direction::Downstream),
        ];
        for (index, (code, direction)) in cases.into_iter().enumerate() {
            let expected_node = node(index as u64 + 1);
            let expected_span = span(index as u32 + 1);
            let (diagnostic, frontier) = localized_unknown_boundary(
                code.clone(),
                "localized failure",
                expected_node.clone(),
                Some(expected_span.clone()),
                direction,
                "next operation",
            );
            assert_eq!(diagnostic.code, code);
            assert_eq!(diagnostic.snapshot, Some(snapshot()));
            assert_eq!(diagnostic.frontier, Some(expected_node.clone()));
            assert_eq!(diagnostic.span, Some(expected_span));
            assert_eq!(frontier.node, expected_node);
            assert_eq!(frontier.affected_direction, direction);
        }
    }
}
