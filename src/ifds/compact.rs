//! A source/observation projection of the full IFDS evidence report.
//!
//! IDs here are pair-local. The full report remains the authority for graph edges,
//! witnesses and diagnostics; compact references point into that exact report.

use super::branches::BranchPaths;
use super::ir::{EdgeKind, Operation, ProcedureIr};
use super::model::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const COMPACT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    pub report_id: String,
    pub section: String,
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSite {
    pub node: NodeId,
    pub operation: String,
    pub span: SourceSpan,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRole {
    Writer,
    Input,
    Computation,
    Literal,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDefinition {
    pub id: LogicalNodeId,
    pub role: SourceRole,
    pub before: Option<SourceSite>,
    pub after: Option<SourceSite>,
    pub before_upstream: BTreeSet<LogicalNodeId>,
    pub after_upstream: BTreeSet<LogicalNodeId>,
    pub before_assignment_guard: Option<BTreeSet<GuardClause>>,
    pub after_assignment_guard: Option<BTreeSet<GuardClause>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlDefinition {
    pub id: LogicalNodeId,
    pub before: Option<SourceSite>,
    pub after: Option<SourceSite>,
    pub before_value: Option<ConditionIdentity>,
    pub after_value: Option<ConditionIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionIdentity {
    pub read: Option<NodeId>,
    pub binding: Option<BindingId>,
    pub reaching_writes: BTreeSet<DefinitionId>,
    pub origins: BTreeSet<Source>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardTerm {
    pub control: LogicalNodeId,
    pub outcome: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardClause {
    pub terms: BTreeSet<GuardTerm>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSelection {
    pub source: LogicalNodeId,
    /// Disjunction of supported branch-outcome conjunctions. None means the
    /// precise rule is unavailable; an empty clause means unconditional.
    pub clauses: Option<BTreeSet<GuardClause>>,
    pub exact_condition_count: usize,
    pub exact_condition_example: Option<String>,
    pub evidence: BTreeSet<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverwriteRule {
    pub earlier: LogicalNodeId,
    pub later: LogicalNodeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationState {
    pub sources: BTreeSet<LogicalNodeId>,
    pub exhaustive: bool,
    pub selections: Vec<SourceSelection>,
    pub precedence: Vec<OverwriteRule>,
    pub use_guard: Option<BTreeSet<GuardClause>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub id: LogicalNodeId,
    pub projection: String,
    pub before: Option<SourceSite>,
    pub after: Option<SourceSite>,
    pub before_state: Option<ObservationState>,
    pub after_state: Option<ObservationState>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationRef {
    pub id: LogicalNodeId,
    pub projection: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationGroup {
    pub id: u32,
    pub members: Vec<ObservationRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    SourceSetChanged,
    SelectionChanged,
    ExpressionChanged,
    WriteAdded,
    WriteRemoved,
    ObservationAdded,
    ObservationRemoved,
    ComparisonUnresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub kind: FindingKind,
    pub subject: LogicalNodeId,
    pub observation: Option<LogicalNodeId>,
    pub projection: Option<String>,
    pub before_sources: BTreeSet<LogicalNodeId>,
    pub after_sources: BTreeSet<LogicalNodeId>,
    pub before_bindings: BTreeSet<LogicalBindingId>,
    pub after_bindings: BTreeSet<LogicalBindingId>,
    pub changed_operations: BTreeSet<LogicalNodeId>,
    pub evidence: BTreeSet<EvidenceRef>,
}

#[derive(Default)]
struct FindingImpact {
    before_sources: BTreeSet<LogicalNodeId>,
    after_sources: BTreeSet<LogicalNodeId>,
    before_bindings: BTreeSet<LogicalBindingId>,
    after_bindings: BTreeSet<LogicalBindingId>,
    changed_operations: BTreeSet<LogicalNodeId>,
}

type FindingKey = (
    FindingKind,
    LogicalNodeId,
    Option<LogicalNodeId>,
    Option<String>,
);
type ObservationFactsKey = (String, String, Option<String>, Option<String>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariableSourceReport {
    pub schema_version: u32,
    pub analysis_version: String,
    pub full_report_id: String,
    pub evidence_file: String,
    pub snapshots: BTreeSet<SnapshotId>,
    pub selected_binding: BindingSelector,
    pub entry_assumptions: BTreeSet<EntryAssumption>,
    pub declared_scope: String,
    pub sources: Vec<SourceDefinition>,
    pub controls: Vec<ControlDefinition>,
    pub observations: Vec<Observation>,
    pub observation_groups: Vec<ObservationGroup>,
    pub findings: Vec<Finding>,
    pub analysis_coverage: Completeness,
    pub comparison_complete: bool,
    pub presentation_complete: bool,
    pub omitted_groups: usize,
    pub diagnostics: BTreeSet<DiagnosticCode>,
    pub source_boundaries: BTreeSet<String>,
    pub sink_boundaries: BTreeSet<String>,
}

impl VariableSourceReport {
    pub fn from_json(bytes: &[u8]) -> Result<Self, SchemaError> {
        let report: Self = serde_json::from_slice(bytes)
            .map_err(|error| SchemaError::Serialization(error.to_string()))?;
        if report.schema_version != COMPACT_SCHEMA_VERSION {
            return Err(SchemaError::UnsupportedVersion {
                found: report.schema_version,
                supported: COMPACT_SCHEMA_VERSION,
            });
        }
        Ok(report)
    }

    pub fn validate_with_full(&self, full: &VariableFlowReport) -> Result<(), SchemaError> {
        if self.schema_version != COMPACT_SCHEMA_VERSION {
            return Err(SchemaError::UnsupportedVersion {
                found: self.schema_version,
                supported: COMPACT_SCHEMA_VERSION,
            });
        }
        if self.full_report_id != report_id(full) || self.snapshots != full.snapshots {
            return Err(SchemaError::Serialization(
                "compact report does not match full evidence".into(),
            ));
        }
        let references = self
            .sources
            .iter()
            .flat_map(|source| {
                [source.before.as_ref(), source.after.as_ref()]
                    .into_iter()
                    .flatten()
                    .map(|site| &site.evidence)
            })
            .chain(self.controls.iter().flat_map(|control| {
                [control.before.as_ref(), control.after.as_ref()]
                    .into_iter()
                    .flatten()
                    .map(|site| &site.evidence)
            }))
            .chain(self.observations.iter().flat_map(|observation| {
                [observation.before.as_ref(), observation.after.as_ref()]
                    .into_iter()
                    .flatten()
                    .map(|site| &site.evidence)
            }))
            .chain(
                self.observations
                    .iter()
                    .flat_map(|observation| {
                        [
                            observation.before_state.as_ref(),
                            observation.after_state.as_ref(),
                        ]
                        .into_iter()
                        .flatten()
                    })
                    .flat_map(|state| state.selections.iter())
                    .flat_map(|selection| &selection.evidence),
            )
            .chain(self.findings.iter().flat_map(|finding| &finding.evidence));
        for reference in references {
            if reference.report_id != self.full_report_id
                || !match reference.section.as_str() {
                    "before_graph.nodes" => reference.index < full.before_graph.nodes.len(),
                    "after_graph.nodes" => reference.index < full.after_graph.nodes.len(),
                    "before_graph.edges" => reference.index < full.before_graph.edges.len(),
                    "after_graph.edges" => reference.index < full.after_graph.edges.len(),
                    "deltas" => reference.index < full.deltas.len(),
                    _ => false,
                }
            {
                return Err(SchemaError::Serialization(format!(
                    "invalid compact evidence reference: {reference:?}"
                )));
            }
        }
        Ok(())
    }

    pub fn to_canonical_json(&self) -> Result<Vec<u8>, SchemaError> {
        if self.schema_version != COMPACT_SCHEMA_VERSION {
            return Err(SchemaError::UnsupportedVersion {
                found: self.schema_version,
                supported: COMPACT_SCHEMA_VERSION,
            });
        }
        serde_json::to_vec(self).map_err(|error| SchemaError::Serialization(error.to_string()))
    }

    pub fn render_text(&self) -> String {
        let sites: BTreeMap<_, _> = self
            .sources
            .iter()
            .map(|source| (source.id, source))
            .collect();
        let controls: BTreeMap<_, _> = self
            .controls
            .iter()
            .map(|guard| (guard.id, guard))
            .collect();
        let mut lines = Vec::new();
        let name = self
            .selected_binding
            .expected_name
            .as_deref()
            .unwrap_or("selected binding");
        for group in &self.observation_groups {
            let Some(observation) = group.members.iter().find_map(|member| {
                self.observations.iter().find(|observation| {
                    observation.id == member.id && observation.projection == member.projection
                })
            }) else {
                continue;
            };
            let affected = self.findings.iter().any(|finding| {
                group
                    .members
                    .iter()
                    .any(|member| finding.observation == Some(member.id))
                    || (group
                        .members
                        .iter()
                        .any(|member| finding.subject == member.id)
                        && matches!(
                            finding.kind,
                            FindingKind::ObservationAdded | FindingKind::ObservationRemoved
                        ))
            });
            if !affected {
                continue;
            }
            let location = observation.after.as_ref().or(observation.before.as_ref());
            let Some(location) = location else {
                continue;
            };
            lines.push(format!(
                "{name} at {} ({}:{}, input {})",
                location.operation,
                location.span.path,
                location.span.start_line,
                observation.projection
            ));
            if group.members.len() > 1 {
                let others: Vec<_> = group
                    .members
                    .iter()
                    .skip(1)
                    .filter_map(|member| {
                        self.observations.iter().find(|observation| {
                            observation.id == member.id
                                && observation.projection == member.projection
                        })
                    })
                    .filter_map(|observation| {
                        observation.after.as_ref().or(observation.before.as_ref())
                    })
                    .map(|site| {
                        format!(
                            "{}:{} ({})",
                            site.span.path, site.span.start_line, site.operation
                        )
                    })
                    .collect();
                if !others.is_empty() {
                    lines.push(format!("Also at: {}.", others.join("; ")));
                }
            }
            if let Some(state) = &observation.before_state {
                lines.push(format!(
                    "Before: {}.",
                    render_sources(state, SnapshotSide::Before, &sites, &controls)
                ));
            }
            if let Some(state) = &observation.after_state {
                lines.push(format!(
                    "After: {}.",
                    render_sources(state, SnapshotSide::After, &sites, &controls)
                ));
                for selection in &state.selections {
                    if !state.precedence.is_empty() && selection.clauses.is_some() {
                        continue;
                    }
                    if selection.clauses.as_ref()
                        == sites
                            .get(&selection.source)
                            .and_then(|source| source.after_assignment_guard.as_ref())
                    {
                        continue;
                    }
                    if let Some(clauses) = &selection.clauses {
                        if clauses
                            == &BTreeSet::from([GuardClause {
                                terms: BTreeSet::new(),
                            }])
                        {
                            continue;
                        }
                        let label = site_label(selection.source, SnapshotSide::After, &sites);
                        lines.push(format!(
                            "Selection for {label}: {}.",
                            render_clauses(clauses, &controls, SnapshotSide::After)
                        ));
                    } else {
                        let label = site_label(selection.source, SnapshotSide::After, &sites);
                        lines.push(format!(
                            "Selection for {label}: compact rule unresolved; see full evidence."
                        ));
                    }
                }
                for rule in &state.precedence {
                    lines.push(format!(
                        "Precedence: {} overrides {} when both writes apply.",
                        site_label(rule.later, SnapshotSide::After, &sites),
                        site_label(rule.earlier, SnapshotSide::After, &sites)
                    ));
                }
                if let Some(use_guard) = &state.use_guard
                    && use_guard
                        != &BTreeSet::from([GuardClause {
                            terms: BTreeSet::new(),
                        }])
                {
                    lines.push(format!(
                        "Use guard: {}.",
                        render_clauses(use_guard, &controls, SnapshotSide::After)
                    ));
                }
            }
            for finding in self.findings.iter().filter(|finding| {
                finding.observation == Some(observation.id)
                    && finding.projection.as_deref() == Some(&observation.projection)
            }) {
                if finding.kind == FindingKind::ExpressionChanged {
                    for changed in &finding.changed_operations {
                        lines.push(format!(
                            "Expression changed: {} -> {}.",
                            site_label(*changed, SnapshotSide::Before, &sites),
                            site_label(*changed, SnapshotSide::After, &sites)
                        ));
                    }
                }
            }
        }
        for finding in &self.findings {
            if finding.observation.is_some() {
                continue;
            }
            if matches!(
                finding.kind,
                FindingKind::WriteAdded | FindingKind::WriteRemoved
            ) && self.observations.iter().any(|observation| {
                self.findings
                    .iter()
                    .any(|change| change.observation == Some(observation.id))
                    && [
                        observation.before_state.as_ref(),
                        observation.after_state.as_ref(),
                    ]
                    .into_iter()
                    .flatten()
                    .any(|state| state.sources.contains(&finding.subject))
            }) {
                continue;
            }
            let side = if matches!(
                finding.kind,
                FindingKind::WriteRemoved | FindingKind::ObservationRemoved
            ) {
                SnapshotSide::Before
            } else {
                SnapshotSide::After
            };
            let label = site_label(finding.subject, side, &sites);
            match finding.kind {
                FindingKind::WriteAdded => lines.push(format!("Added write {label}.")),
                FindingKind::WriteRemoved => lines.push(format!("Removed write {label}.")),
                FindingKind::ExpressionChanged => {
                    lines.push(format!("Changed expression at {label}."))
                }
                FindingKind::ObservationAdded => lines.push(format!("Added observation {label}.")),
                FindingKind::ObservationRemoved => {
                    lines.push(format!("Removed observation {label}."))
                }
                FindingKind::ComparisonUnresolved => {
                    lines.push("Comparison unresolved at one or more aligned operations.".into())
                }
                _ => {}
            }
        }
        if lines.is_empty() {
            lines.push("No established source or logic change within the declared scope.".into());
        }
        if !self.diagnostics.is_empty() {
            lines.push(format!("Unknown: {:?}.", self.diagnostics));
        }
        lines.push(format!(
            "Scope: {}; analysis {:?}; source comparison {}. Full evidence: {} ({}).",
            self.declared_scope,
            self.analysis_coverage,
            if self.comparison_complete {
                "complete"
            } else {
                "unresolved"
            },
            self.evidence_file,
            self.full_report_id
        ));
        lines.join("\n")
    }
}

fn site_label(
    id: LogicalNodeId,
    side: SnapshotSide,
    sites: &BTreeMap<LogicalNodeId, &SourceDefinition>,
) -> String {
    sites
        .get(&id)
        .and_then(|source| match side {
            SnapshotSide::Before => source.before.as_ref(),
            SnapshotSide::After => source.after.as_ref(),
        })
        .map(|site| site.operation.clone())
        .unwrap_or_else(|| format!("source {}", id.0))
}

fn render_sources(
    state: &ObservationState,
    side: SnapshotSide,
    sites: &BTreeMap<LogicalNodeId, &SourceDefinition>,
    controls: &BTreeMap<LogicalNodeId, &ControlDefinition>,
) -> String {
    let mut labels: Vec<_> = state
        .sources
        .iter()
        .map(|id| {
            let label = site_label(*id, side, sites);
            let guard = sites.get(id).and_then(|source| match side {
                SnapshotSide::Before => source.before_assignment_guard.as_ref(),
                SnapshotSide::After => source.after_assignment_guard.as_ref(),
            });
            if let Some(guard) = guard {
                if guard
                    == &BTreeSet::from([GuardClause {
                        terms: BTreeSet::new(),
                    }])
                {
                    if state.sources.len() > 1 {
                        return format!("{label} as fallback");
                    }
                } else {
                    return format!("{label} under {}", render_clauses(guard, controls, side));
                }
            }
            label
        })
        .collect();
    if !state.exhaustive {
        labels.push("other sources unresolved".into());
    }
    if labels.is_empty() {
        "no reaching source established".into()
    } else {
        labels.join("; ")
    }
}

fn render_clauses(
    clauses: &BTreeSet<GuardClause>,
    controls: &BTreeMap<LogicalNodeId, &ControlDefinition>,
    side: SnapshotSide,
) -> String {
    clauses
        .iter()
        .map(|clause| {
            if clause.terms.is_empty() {
                return "always".into();
            }
            clause
                .terms
                .iter()
                .map(|term| {
                    let label = controls
                        .get(&term.control)
                        .and_then(|control| match side {
                            SnapshotSide::Before => control.before.as_ref(),
                            SnapshotSide::After => control.after.as_ref(),
                        })
                        .map(|site| site.operation.as_str())
                        .unwrap_or("unresolved guard");
                    let expression = label
                        .strip_prefix("branch `")
                        .and_then(|value| value.strip_suffix('`'))
                        .unwrap_or(label);
                    let expression = expression
                        .strip_prefix('(')
                        .and_then(|value| value.strip_suffix(')'))
                        .unwrap_or(expression);
                    if term.outcome {
                        expression.to_owned()
                    } else if expression
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
                    {
                        format!("!{expression}")
                    } else {
                        format!("!({expression})")
                    }
                })
                .collect::<Vec<_>>()
                .join(" && ")
        })
        .collect::<Vec<_>>()
        .join(" || ")
}

fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn observation_state_key(state: &Option<ObservationState>) -> String {
    let Some(state) = state else {
        return "absent".into();
    };
    let selections: Vec<_> = state
        .selections
        .iter()
        .map(|selection| {
            (
                selection.source,
                &selection.clauses,
                selection.exact_condition_count,
                &selection.exact_condition_example,
            )
        })
        .collect();
    serde_json::to_string(&(
        state.sources.clone(),
        state.exhaustive,
        selections,
        &state.precedence,
        &state.use_guard,
    ))
    .expect("compact state serializes")
}

fn report_id(full: &VariableFlowReport) -> String {
    let mut identity = full.clone();
    identity.human_summary.clear();
    let bytes = serde_json::to_vec(&identity).expect("validated report serializes");
    format!("ifds-{:016x}", stable_hash(&bytes))
}

fn value_relation(relation: &RelationKind) -> bool {
    matches!(
        relation,
        RelationKind::Reaches
            | RelationKind::ValueDependency
            | RelationKind::Argument
            | RelationKind::Return
    )
}

fn site(node: &NodeId, graph: &FlowGraph, section: &str, identity: &str) -> Option<SourceSite> {
    graph
        .nodes
        .iter()
        .enumerate()
        .find(|(_, item)| &item.id == node)
        .map(|(index, item)| SourceSite {
            node: node.clone(),
            operation: item.operation.clone(),
            span: item.span.clone(),
            evidence: EvidenceRef {
                report_id: identity.into(),
                section: section.into(),
                index,
            },
        })
}

fn role(
    alignment: &Alignment,
    before_ir: &ProcedureIr,
    after_ir: &ProcedureIr,
    before: Option<&SourceSite>,
    after: Option<&SourceSite>,
) -> SourceRole {
    let operation = alignment
        .after
        .as_ref()
        .and_then(|id| after_ir.nodes.get(id))
        .or_else(|| {
            alignment
                .before
                .as_ref()
                .and_then(|id| before_ir.nodes.get(id))
        })
        .map(|node| &node.operation);
    match operation {
        Some(Operation::Write { .. }) => SourceRole::Writer,
        Some(Operation::Literal { .. }) => SourceRole::Literal,
        Some(Operation::Compute { .. }) => SourceRole::Computation,
        _ if after
            .or(before)
            .is_some_and(|site| site.operation.starts_with("function_input")) =>
        {
            SourceRole::Input
        }
        _ => SourceRole::Other,
    }
}

fn condition_identity(
    ir: &ProcedureIr,
    paths: &BranchPaths,
    branch: &NodeId,
) -> Option<ConditionIdentity> {
    fn source_read(ir: &ProcedureIr, place: &Place) -> Option<(NodeId, BindingId)> {
        let Place::Temporary(id) = place else {
            return None;
        };
        match &ir.nodes.get(id)?.operation {
            Operation::Read {
                source: Place::Binding(binding),
                ..
            } => Some((id.clone(), binding.clone())),
            Operation::Compute {
                inputs,
                operator: super::ir::PrimitiveOperator::LogicalNot,
                ..
            } => source_read(ir, &inputs.first()?.place),
            _ => None,
        }
    }
    let Operation::Branch { condition } = &ir.nodes.get(branch)?.operation else {
        return None;
    };
    let (read, binding) = source_read(ir, condition)?;
    let facts = paths.at.get(branch)?.iter().flat_map(|state| &state.facts);
    let mut reaching_writes = BTreeSet::new();
    let mut origins = BTreeSet::new();
    for fact in facts {
        match fact {
            Fact::LastWrite {
                place: Place::Binding(candidate),
                write,
            } if candidate == &binding => {
                reaching_writes.insert(write.clone());
            }
            Fact::Origin {
                place: Place::Binding(candidate),
                source,
            } if candidate == &binding => {
                origins.insert(source.clone());
            }
            _ => {}
        }
    }
    Some(ConditionIdentity {
        read: Some(read),
        binding: Some(binding),
        reaching_writes,
        origins,
    })
}

fn clauses(
    paths: &BranchPaths,
    at: &NodeId,
    ids: &BTreeMap<NodeId, LogicalNodeId>,
    predicate: impl Fn(&BTreeSet<Fact>) -> bool,
) -> Option<BTreeSet<GuardClause>> {
    let states = paths.at.get(at)?;
    let relevant: Vec<_> = states
        .iter()
        .filter(|state| predicate(&state.facts))
        .collect();
    if relevant.is_empty() || relevant.iter().any(|state| state.unproven) {
        return None;
    }
    let mut result = BTreeSet::new();
    for state in relevant {
        let terms = state
            .decisions
            .iter()
            .map(|(node, outcome)| {
                Some(GuardTerm {
                    control: *ids.get(node)?,
                    outcome: *outcome,
                })
            })
            .collect::<Option<BTreeSet<_>>>()?;
        result.insert(GuardClause { terms });
    }
    let simplified = simplify_clauses(result);
    (simplified.len() <= 64).then_some(simplified)
}

fn simplify_clauses(mut clauses: BTreeSet<GuardClause>) -> BTreeSet<GuardClause> {
    loop {
        let before = clauses.clone();
        let mut consensus: BTreeMap<(LogicalNodeId, BTreeSet<GuardTerm>), u8> = BTreeMap::new();
        for clause in &before {
            for term in &clause.terms {
                let mut rest = clause.terms.clone();
                rest.remove(term);
                let mask = consensus.entry((term.control, rest)).or_default();
                *mask |= if term.outcome { 1 } else { 2 };
            }
        }
        for ((_, terms), mask) in consensus {
            if mask == 3 {
                clauses.insert(GuardClause { terms });
            }
        }
        let all = clauses.clone();
        clauses.retain(|clause| {
            !all.iter()
                .any(|other| other != clause && other.terms.is_subset(&clause.terms))
        });
        if clauses == before {
            return clauses;
        }
    }
}

fn reachable(ir: &ProcedureIr, from: &NodeId, to: &NodeId) -> bool {
    let mut pending = vec![from.clone()];
    let mut seen = BTreeSet::new();
    while let Some(node) = pending.pop() {
        if !seen.insert(node.clone()) {
            continue;
        }
        if &node == to {
            return true;
        }
        pending.extend(
            ir.edges
                .iter()
                .filter(|edge| {
                    edge.source == node
                        && matches!(edge.kind, EdgeKind::Normal | EdgeKind::Branch { .. })
                })
                .map(|edge| edge.target.clone()),
        );
    }
    false
}

/// Project one exact full report. The IR inputs supply typed branch outcomes and
/// CFG order; display condition strings in the full graph are never parsed.
pub fn project_variable_sources(
    full: &VariableFlowReport,
    before_ir: &ProcedureIr,
    after_ir: &ProcedureIr,
) -> VariableSourceReport {
    let identity = report_id(full);
    let evidence_file = format!("ifds-evidence-{identity}.json");
    let before_ids: BTreeMap<_, _> = full
        .alignment
        .iter()
        .filter_map(|a| a.before.as_ref().map(|id| (id.clone(), a.logical)))
        .collect();
    let after_ids: BTreeMap<_, _> = full
        .alignment
        .iter()
        .filter_map(|a| a.after.as_ref().map(|id| (id.clone(), a.logical)))
        .collect();
    let before_paths = BranchPaths::build(
        before_ir,
        super::provenance::seed_entry_facts(before_ir),
        4096,
    );
    let after_paths = BranchPaths::build(
        after_ir,
        super::provenance::seed_entry_facts(after_ir),
        4096,
    );
    let mut sources = Vec::new();
    let mut controls = Vec::new();
    for alignment in &full.alignment {
        let before = alignment
            .before
            .as_ref()
            .and_then(|id| site(id, &full.before_graph, "before_graph.nodes", &identity));
        let after = alignment
            .after
            .as_ref()
            .and_then(|id| site(id, &full.after_graph, "after_graph.nodes", &identity));
        if before.is_none() && after.is_none() {
            continue;
        }
        let is_control = alignment.before.as_ref().is_some_and(|id| {
            before_ir
                .nodes
                .get(id)
                .is_some_and(|n| matches!(n.operation, Operation::Branch { .. }))
        }) || alignment.after.as_ref().is_some_and(|id| {
            after_ir
                .nodes
                .get(id)
                .is_some_and(|n| matches!(n.operation, Operation::Branch { .. }))
        });
        if is_control {
            controls.push(ControlDefinition {
                id: alignment.logical,
                before_value: alignment
                    .before
                    .as_ref()
                    .and_then(|id| condition_identity(before_ir, &before_paths, id)),
                after_value: alignment
                    .after
                    .as_ref()
                    .and_then(|id| condition_identity(after_ir, &after_paths, id)),
                before,
                after,
            });
            continue;
        }
        let upstream =
            |graph: &FlowGraph, ids: &BTreeMap<NodeId, LogicalNodeId>, node: &Option<NodeId>| {
                node.as_ref()
                    .map(|node| {
                        graph
                            .edges
                            .iter()
                            .filter(|edge| &edge.target == node && value_relation(&edge.relation))
                            .filter_map(|edge| ids.get(&edge.source).copied())
                            .collect()
                    })
                    .unwrap_or_default()
            };
        let assignment_guard =
            |node: &Option<NodeId>,
             ir: &ProcedureIr,
             paths: &BranchPaths,
             ids: &BTreeMap<NodeId, LogicalNodeId>| {
                let node = node.as_ref()?;
                matches!(
                    ir.nodes.get(node).map(|n| &n.operation),
                    Some(Operation::Write { .. })
                )
                .then(|| clauses(paths, node, ids, |_| true))
                .flatten()
            };
        sources.push(SourceDefinition {
            id: alignment.logical,
            role: role(
                alignment,
                before_ir,
                after_ir,
                before.as_ref(),
                after.as_ref(),
            ),
            before_upstream: upstream(&full.before_graph, &before_ids, &alignment.before),
            after_upstream: upstream(&full.after_graph, &after_ids, &alignment.after),
            before_assignment_guard: assignment_guard(
                &alignment.before,
                before_ir,
                &before_paths,
                &before_ids,
            ),
            after_assignment_guard: assignment_guard(
                &alignment.after,
                after_ir,
                &after_paths,
                &after_ids,
            ),
            before,
            after,
        });
    }
    let mut observations = Vec::new();
    let mut keys = BTreeSet::new();
    for (graph, ids) in [
        (&full.before_graph, &before_ids),
        (&full.after_graph, &after_ids),
    ] {
        for edge in &graph.edges {
            if value_relation(&edge.relation)
                && let Some(id) = ids.get(&edge.target)
            {
                keys.insert((
                    *id,
                    edge.projection.clone().unwrap_or_else(|| "value".into()),
                ));
            }
        }
    }
    for (id, projection) in keys {
        let Some(alignment) = full
            .alignment
            .iter()
            .find(|alignment| alignment.logical == id)
        else {
            continue;
        };
        let build_state = |side: SnapshotSide, node: &Option<NodeId>| -> Option<ObservationState> {
            let node = node.as_ref()?;
            let (graph, ir, ids, paths, extent) = match side {
                SnapshotSide::Before => (
                    &full.before_graph,
                    before_ir,
                    &before_ids,
                    &before_paths,
                    &full.flow_extent.before,
                ),
                SnapshotSide::After => (
                    &full.after_graph,
                    after_ir,
                    &after_ids,
                    &after_paths,
                    &full.flow_extent.after,
                ),
            };
            if !graph.nodes.iter().any(|item| &item.id == node) {
                return None;
            }
            let incoming: Vec<_> = graph
                .edges
                .iter()
                .enumerate()
                .filter(|(_, edge)| {
                    &edge.target == node
                        && value_relation(&edge.relation)
                        && edge.projection.as_deref().unwrap_or("value") == projection
                })
                .collect();
            let source_ids: BTreeSet<_> = incoming
                .iter()
                .filter_map(|(_, edge)| ids.get(&edge.source).copied())
                .collect();
            let mut selections = Vec::new();
            for source in &source_ids {
                let edges: Vec<_> = incoming
                    .iter()
                    .filter(|(_, edge)| ids.get(&edge.source) == Some(source))
                    .collect();
                let exact_conditions: BTreeSet<_> = edges
                    .iter()
                    .filter_map(|(_, edge)| edge.condition.clone())
                    .collect();
                let evidence = edges
                    .iter()
                    .map(|(index, _)| EvidenceRef {
                        report_id: identity.clone(),
                        section: if side == SnapshotSide::Before {
                            "before_graph.edges"
                        } else {
                            "after_graph.edges"
                        }
                        .into(),
                        index: *index,
                    })
                    .collect();
                let source_node = edges.first().map(|(_, edge)| &edge.source);
                let typed = source_node.and_then(|source_node| ir.nodes.get(source_node)).and_then(|source_node| {
                    match &source_node.operation {
                        Operation::Write { target, definition, .. } => clauses(paths, node, ids, |facts| {
                            facts.contains(&Fact::LastWrite { place: target.clone(), write: definition.clone() })
                        }),
                        Operation::Literal { definition, .. } | Operation::Compute { definition, .. } =>
                            clauses(paths, node, ids, |facts| facts.iter().any(|fact| matches!(fact,
                                Fact::Origin { source: Source::Write(candidate), .. } if candidate == definition))),
                        _ => clauses(paths, node, ids, |_| true),
                    }
                });
                selections.push(SourceSelection {
                    source: *source,
                    clauses: typed,
                    exact_condition_count: exact_conditions.len(),
                    exact_condition_example: exact_conditions.iter().next().cloned(),
                    evidence,
                });
            }
            let mut precedence = Vec::new();
            for earlier in &source_ids {
                for later in &source_ids {
                    if earlier == later {
                        continue;
                    }
                    let a = incoming
                        .iter()
                        .find(|(_, edge)| ids.get(&edge.source) == Some(earlier))
                        .map(|(_, edge)| &edge.source);
                    let b = incoming
                        .iter()
                        .find(|(_, edge)| ids.get(&edge.source) == Some(later))
                        .map(|(_, edge)| &edge.source);
                    if let (Some(a), Some(b)) = (a, b)
                        && matches!((ir.nodes.get(a).map(|n| &n.operation), ir.nodes.get(b).map(|n| &n.operation)),
                            (Some(Operation::Write { target: first, .. }), Some(Operation::Write { target: second, .. })) if first == second)
                        && reachable(ir, a, b)
                        && !reachable(ir, b, a)
                    {
                        precedence.push(OverwriteRule {
                            earlier: *earlier,
                            later: *later,
                        });
                    }
                }
            }
            let use_guard = clauses(paths, node, ids, |_| true);
            Some(ObservationState {
                sources: source_ids,
                exhaustive: extent.upstream == Coverage::Complete
                    && extent.downstream == Coverage::Complete
                    && !incoming
                        .iter()
                        .any(|(_, edge)| matches!(edge.evidence, EvidenceKind::Unresolved { .. })),
                selections,
                precedence,
                use_guard,
            })
        };
        observations.push(Observation {
            id,
            projection: projection.clone(),
            before: alignment
                .before
                .as_ref()
                .and_then(|node| site(node, &full.before_graph, "before_graph.nodes", &identity)),
            after: alignment
                .after
                .as_ref()
                .and_then(|node| site(node, &full.after_graph, "after_graph.nodes", &identity)),
            before_state: build_state(SnapshotSide::Before, &alignment.before),
            after_state: build_state(SnapshotSide::After, &alignment.after),
        });
    }
    let mut groups_by_facts: BTreeMap<ObservationFactsKey, Vec<ObservationRef>> = BTreeMap::new();
    for observation in &observations {
        groups_by_facts
            .entry((
                observation_state_key(&observation.before_state),
                observation_state_key(&observation.after_state),
                observation
                    .before
                    .as_ref()
                    .map(|site| site.operation.clone()),
                observation
                    .after
                    .as_ref()
                    .map(|site| site.operation.clone()),
            ))
            .or_default()
            .push(ObservationRef {
                id: observation.id,
                projection: observation.projection.clone(),
            });
    }
    let observation_groups: Vec<_> = groups_by_facts
        .into_values()
        .enumerate()
        .map(|(index, members)| ObservationGroup {
            id: index as u32 + 1,
            members,
        })
        .collect();
    let mut findings: BTreeMap<FindingKey, BTreeSet<EvidenceRef>> = BTreeMap::new();
    let mut impacts: BTreeMap<FindingKey, FindingImpact> = BTreeMap::new();
    let by_node: BTreeMap<_, _> = full
        .alignment
        .iter()
        .flat_map(|a| {
            a.before
                .iter()
                .chain(a.after.iter())
                .map(move |node| (node.clone(), a.logical))
        })
        .collect();
    for (index, record) in full.deltas.iter().enumerate() {
        let evidence = EvidenceRef {
            report_id: identity.clone(),
            section: "deltas".into(),
            index,
        };
        let key = match &record.delta {
            FlowDelta::ValueSourceChanged {
                consumer,
                projection,
                before_sources,
                after_sources,
                before_bindings,
                after_bindings,
                changed_operations,
                ..
            } => {
                let kind = if before_sources == after_sources
                    && before_bindings == after_bindings
                    && !changed_operations.is_empty()
                {
                    FindingKind::ExpressionChanged
                } else {
                    FindingKind::SourceSetChanged
                };
                let key = (kind, *consumer, Some(*consumer), Some(projection.clone()));
                impacts.insert(
                    key.clone(),
                    FindingImpact {
                        before_sources: before_sources.clone(),
                        after_sources: after_sources.clone(),
                        before_bindings: before_bindings.clone(),
                        after_bindings: after_bindings.clone(),
                        changed_operations: changed_operations.clone(),
                    },
                );
                Some(key)
            }
            FlowDelta::FlowConditionChanged {
                target, projection, ..
            } => Some((
                FindingKind::SelectionChanged,
                *target,
                Some(*target),
                projection.clone(),
            )),
            FlowDelta::OperationChanged { logical, .. } => {
                Some((FindingKind::ExpressionChanged, *logical, None, None))
            }
            FlowDelta::WriteAdded { after } => by_node
                .get(after)
                .map(|id| (FindingKind::WriteAdded, *id, None, None)),
            FlowDelta::WriteRemoved { before } => by_node
                .get(before)
                .map(|id| (FindingKind::WriteRemoved, *id, None, None)),
            FlowDelta::NodeAdded { after }
                if matches!(
                    after_ir.nodes.get(after).map(|n| &n.operation),
                    Some(Operation::Return { .. })
                ) =>
            {
                by_node
                    .get(after)
                    .map(|id| (FindingKind::ObservationAdded, *id, None, None))
            }
            FlowDelta::NodeRemoved { before }
                if matches!(
                    before_ir.nodes.get(before).map(|n| &n.operation),
                    Some(Operation::Return { .. })
                ) =>
            {
                by_node
                    .get(before)
                    .map(|id| (FindingKind::ObservationRemoved, *id, None, None))
            }
            FlowDelta::AnalysisUnknown { .. } => Some((
                FindingKind::ComparisonUnresolved,
                LogicalNodeId(0),
                None,
                None,
            )),
            _ => None,
        };
        if let Some(key) = key {
            findings.entry(key).or_default().insert(evidence);
        }
    }
    let source_findings: Vec<_> = impacts.keys().cloned().collect();
    for key in source_findings {
        let impact = &impacts[&key];
        let related: BTreeSet<_> = impact
            .before_sources
            .union(&impact.after_sources)
            .copied()
            .chain(impact.changed_operations.iter().copied())
            .collect();
        for (index, record) in full.deltas.iter().enumerate() {
            let related_delta = match &record.delta {
                FlowDelta::NodeAdded { after } | FlowDelta::WriteAdded { after } => {
                    by_node.get(after).is_some_and(|id| related.contains(id))
                }
                FlowDelta::NodeRemoved { before } | FlowDelta::WriteRemoved { before } => {
                    by_node.get(before).is_some_and(|id| related.contains(id))
                }
                FlowDelta::OperationChanged { logical, .. } => related.contains(logical),
                FlowDelta::FlowAdded { source, target, .. }
                | FlowDelta::FlowRemoved { source, target, .. }
                | FlowDelta::FlowConditionChanged { source, target, .. } => {
                    *target == key.1 && related.contains(source)
                }
                _ => false,
            };
            if related_delta {
                findings
                    .entry(key.clone())
                    .or_default()
                    .insert(EvidenceRef {
                        report_id: identity.clone(),
                        section: "deltas".into(),
                        index,
                    });
            }
        }
        findings.retain(|other, _| {
            other == &key
                || !(other.2.is_none()
                    && matches!(
                        other.0,
                        FindingKind::WriteAdded
                            | FindingKind::WriteRemoved
                            | FindingKind::ExpressionChanged
                    )
                    && related.contains(&other.1))
        });
    }
    for observation in &observations {
        if observation
            .before_state
            .as_ref()
            .map(|state| &state.sources)
            != observation.after_state.as_ref().map(|state| &state.sources)
        {
            continue;
        }
        let selection_signature = |state: &ObservationState| {
            state
                .selections
                .iter()
                .map(|selection| (selection.source, selection.clauses.clone()))
                .collect::<Vec<_>>()
        };
        if observation.before_state.as_ref().map(&selection_signature)
            != observation.after_state.as_ref().map(&selection_signature)
            || observation
                .before_state
                .as_ref()
                .map(|state| &state.precedence)
                != observation
                    .after_state
                    .as_ref()
                    .map(|state| &state.precedence)
            || observation
                .before_state
                .as_ref()
                .map(|state| &state.use_guard)
                != observation
                    .after_state
                    .as_ref()
                    .map(|state| &state.use_guard)
        {
            // Branch and CFG evidence can expose changed selection even when a
            // string-based relation comparison did not establish equivalence.
            let evidence = observation
                .before_state
                .iter()
                .chain(observation.after_state.iter())
                .flat_map(|state| state.selections.iter())
                .flat_map(|selection| selection.evidence.iter().cloned())
                .collect::<BTreeSet<_>>();
            findings
                .entry((
                    FindingKind::SelectionChanged,
                    observation.id,
                    Some(observation.id),
                    Some(observation.projection.clone()),
                ))
                .or_default()
                .extend(evidence);
        }
    }
    let findings = findings
        .into_iter()
        .map(|(key, evidence)| {
            let impact = impacts.remove(&key).unwrap_or_default();
            let (kind, subject, observation, projection) = key;
            Finding {
                kind,
                subject,
                observation,
                projection,
                before_sources: impact.before_sources,
                after_sources: impact.after_sources,
                before_bindings: impact.before_bindings,
                after_bindings: impact.after_bindings,
                changed_operations: impact.changed_operations,
                evidence,
            }
        })
        .collect();
    let diagnostics: BTreeSet<_> = full.diagnostics.iter().map(|d| d.code.clone()).collect();
    let comparison_complete = full.completeness == Completeness::CompleteForQuery
        && !diagnostics.contains(&DiagnosticCode::AmbiguousMatch)
        && !full
            .deltas
            .iter()
            .any(|record| matches!(record.delta, FlowDelta::AnalysisUnknown { .. }));
    VariableSourceReport {
        schema_version: COMPACT_SCHEMA_VERSION,
        analysis_version: full.analysis_version.clone(),
        full_report_id: identity,
        evidence_file,
        snapshots: full.snapshots.clone(),
        selected_binding: full.query.request.selected_binding.clone(),
        entry_assumptions: full.entry_assumptions.clone(),
        declared_scope: full.flow_extent.after.declared_scope.clone(),
        sources,
        controls,
        observations,
        observation_groups,
        findings,
        analysis_coverage: full.completeness,
        comparison_complete,
        presentation_complete: true,
        omitted_groups: 0,
        diagnostics,
        source_boundaries: full
            .flow_extent
            .before
            .source_boundaries
            .union(&full.flow_extent.after.source_boundaries)
            .cloned()
            .collect(),
        sink_boundaries: full
            .flow_extent
            .before
            .sink_boundaries
            .union(&full.flow_extent.after.sink_boundaries)
            .cloned()
            .collect(),
    }
}
