//! Public identities, queries, reports, evidence, and diagnostics.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// The only report schema currently accepted by this crate.
pub const REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotId {
    pub side: SnapshotSide,
    pub revision: String,
    pub content_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotSide {
    Before,
    After,
}

macro_rules! snapshot_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            pub snapshot: SnapshotId,
            pub local: u64,
        }

        impl $name {
            pub fn new(snapshot: SnapshotId, local: u64) -> Self {
                Self { snapshot, local }
            }
        }
    };
}

snapshot_id!(BindingId);
snapshot_id!(NodeId);
snapshot_id!(DefinitionId);
snapshot_id!(CallSiteId);
snapshot_id!(ProcedureId);

macro_rules! logical_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u64);
    };
}

logical_id!(LogicalNodeId);
logical_id!(LogicalBindingId);
logical_id!(GroupId);
logical_id!(WitnessId);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpan {
    pub path: String,
    pub byte_start: u64,
    pub byte_end: u64,
    pub start_line: u32,
    pub end_line: u32,
}

impl SourceSpan {
    pub fn validate(&self) -> Result<(), SchemaError> {
        if self.path.is_empty() {
            return Err(SchemaError::InvalidSpan {
                path: self.path.clone(),
                reason: "path must not be empty".into(),
            });
        }
        if self.byte_start > self.byte_end {
            return Err(SchemaError::InvalidSpan {
                path: self.path.clone(),
                reason: "byte_start exceeds byte_end".into(),
            });
        }
        if self.start_line == 0 || self.start_line > self.end_line {
            return Err(SchemaError::InvalidSpan {
                path: self.path.clone(),
                reason: "lines must be one-based and ordered".into(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Place {
    Binding(BindingId),
    Temporary(NodeId),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Source {
    Write(DefinitionId),
    ParameterEntry(DefinitionId),
    FunctionInput(BindingId),
    ModeledExternal { model: String, input: String },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Fact {
    Zero,
    LastWrite { place: Place, write: DefinitionId },
    Origin { place: Place, source: Source },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotHandle {
    pub id: SnapshotId,
    pub repository_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    Added,
    Deleted,
    Modified,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileChange {
    pub kind: FileChangeKind,
    pub before_path: Option<String>,
    pub after_path: Option<String>,
    pub before_content_id: Option<String>,
    pub after_content_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryDiff {
    pub before_content_id: String,
    pub after_content_id: String,
    pub changes: BTreeSet<FileChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingSelector {
    pub snapshot: SnapshotId,
    pub declaration: SourceSpan,
    pub expected_name: Option<String>,
    pub expected_enclosing_symbol: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KnownValue {
    String { value: String },
    Number { value: String },
    Boolean { value: bool },
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EntryPoint {
    ContainingFunction,
    Call {
        call_site: CallSiteId,
        known_arguments: BTreeMap<u32, KnownValue>,
        enclosing_caller_included: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelVersion {
    pub name: String,
    pub version: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilitySet {
    pub stage: u16,
    pub capabilities: BTreeSet<String>,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisLimits {
    pub time_ms: u64,
    pub memory_bytes: u64,
    pub processed_path_edges: u64,
    pub witnesses_per_relation: u32,
    pub output_nodes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariableFlowQuery {
    pub before: SnapshotHandle,
    pub after: SnapshotHandle,
    pub diff: RepositoryDiff,
    pub selected_binding: BindingSelector,
    pub counterpart: Option<BindingSelector>,
    pub entry: EntryPoint,
    pub capabilities: CapabilitySet,
    pub summaries: BTreeSet<ModelVersion>,
    pub limits: AnalysisLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Reaches,
    ValueDependency,
    Controls,
    Argument,
    Return,
    MayWrite,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Supported,
    Modeled { model: ModelVersion },
    Unresolved { diagnostic: DiagnosticCode },
}

/// Whether the available evidence decides the presence of a particular relation.
///
/// `Absent` is deliberately stronger than an empty result: it is valid only when
/// every relevant alternative was analyzed completely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationPresence {
    Present,
    Absent,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowNode {
    pub id: NodeId,
    pub operation: String,
    pub span: SourceSpan,
    pub enclosing_declaration: Option<String>,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowEdge {
    pub source: NodeId,
    pub target: NodeId,
    pub relation: RelationKind,
    pub projection: Option<String>,
    pub condition: Option<String>,
    pub evidence: EvidenceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowGraph {
    pub nodes: BTreeSet<FlowNode>,
    pub edges: BTreeSet<FlowEdge>,
    pub facts: BTreeSet<Fact>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alignment {
    pub logical: LogicalNodeId,
    pub before: Option<NodeId>,
    pub after: Option<NodeId>,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingAlignment {
    pub logical: LogicalBindingId,
    pub before: Option<BindingId>,
    pub after: Option<BindingId>,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum AlignmentEntity {
    Binding(BindingId),
    Node(NodeId),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlignmentCandidate {
    pub before: AlignmentEntity,
    pub after: AlignmentEntity,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlignmentAmbiguity {
    pub candidates: BTreeSet<AlignmentCandidate>,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipChange {
    Entered,
    Left,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FlowDelta {
    NodeAdded {
        after: NodeId,
    },
    NodeRemoved {
        before: NodeId,
    },
    SliceMembershipChanged {
        logical: LogicalNodeId,
        change: MembershipChange,
    },
    OperationChanged {
        logical: LogicalNodeId,
        before: NodeId,
        after: NodeId,
    },
    WriteAdded {
        after: NodeId,
    },
    WriteRemoved {
        before: NodeId,
    },
    FlowAdded {
        source: LogicalNodeId,
        target: LogicalNodeId,
        relation: RelationKind,
        projection: Option<String>,
    },
    FlowRemoved {
        source: LogicalNodeId,
        target: LogicalNodeId,
        relation: RelationKind,
        projection: Option<String>,
    },
    FlowConditionChanged {
        source: LogicalNodeId,
        target: LogicalNodeId,
        before: String,
        after: String,
    },
    ValueSourceChanged {
        consumer: LogicalNodeId,
        projection: String,
        before_sources: BTreeSet<LogicalNodeId>,
        after_sources: BTreeSet<LogicalNodeId>,
    },
    AnalysisUnknown {
        subject: String,
        side: Option<SnapshotSide>,
        diagnostic: DiagnosticCode,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeltaRecord {
    pub delta: FlowDelta,
    pub groups: BTreeSet<GroupId>,
    pub before_span: Option<SourceSpan>,
    pub after_span: Option<SourceSpan>,
    pub condition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Witness {
    pub id: WitnessId,
    pub nodes: Vec<NodeId>,
    pub backedges: BTreeSet<(NodeId, NodeId)>,
    pub summary_expansions: BTreeSet<String>,
    pub evidence: EvidenceKind,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    UnsupportedSyntax,
    UnresolvedBinding,
    UnresolvedCall,
    UnknownExternalEffect,
    UnknownHeapEffect,
    UnknownSchedule,
    UnprovenPathFeasibility,
    AmbiguousMatch,
    MissingDependency,
    ParseError,
    TypeResolutionUnavailable,
    AccessPathLimit,
    AnalysisBudgetExceeded,
    OutputTruncated,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub message: String,
    pub snapshot: Option<SnapshotId>,
    pub frontier: Option<NodeId>,
    pub span: Option<SourceSpan>,
    pub affected_flows: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnknownFrontier {
    pub node: NodeId,
    pub next_operation: String,
    pub diagnostic: DiagnosticCode,
    pub affected_direction: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Upstream,
    Downstream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Complete,
    Partial,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisRegion {
    pub name: String,
    pub direction: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisLimitKind {
    Time,
    Memory,
    ProcessedPathEdges,
    OutputNodes,
    WitnessesPerRelation,
}

impl AnalysisLimitKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Time => "time",
            Self::Memory => "memory",
            Self::ProcessedPathEdges => "processed_path_edges",
            Self::OutputNodes => "output_nodes",
            Self::WitnessesPerRelation => "witnesses_per_relation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitReason {
    pub kind: AnalysisLimitKind,
    pub limit: u64,
    pub observed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotExtent {
    pub upstream: Coverage,
    pub downstream: Coverage,
    pub declared_scope: String,
    pub source_boundaries: BTreeSet<String>,
    pub sink_boundaries: BTreeSet<String>,
    pub value_lifecycle_closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowExtent {
    pub before: SnapshotExtent,
    pub after: SnapshotExtent,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathEndingKind {
    NoFurtherUse,
    ExecutionExit,
    ScopeBoundary,
    ModeledBoundary,
    UnknownBoundary,
    Cycle,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathEnding {
    pub kind: PathEndingKind,
    pub origin: Source,
    pub carrier: Place,
    pub location: SourceSpan,
    pub condition: Option<String>,
    pub context: String,
    pub witness: WitnessId,
    pub model: Option<ModelVersion>,
    pub diagnostic: Option<DiagnosticCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Completeness {
    CompleteForQuery,
    Partial,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisStats {
    pub elapsed_ms: u64,
    pub peak_memory_bytes: u64,
    pub processed_path_edges: u64,
    pub graph_nodes: u64,
    pub graph_edges: u64,
    pub witnesses_emitted: u64,
    pub witnesses_omitted: u64,
    pub limits_hit: BTreeSet<String>,
    pub graph_truncated: bool,
    pub witnesses_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryAssumption {
    pub name: String,
    pub value: Option<KnownValue>,
    pub source: Source,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedQuery {
    pub request: VariableFlowQuery,
    pub before_binding: Option<BindingId>,
    pub after_binding: Option<BindingId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariableFlowReport {
    pub schema_version: u32,
    pub analysis_version: String,
    pub snapshots: BTreeSet<SnapshotId>,
    pub query: ResolvedQuery,
    pub entry_assumptions: BTreeSet<EntryAssumption>,
    pub before_graph: FlowGraph,
    pub after_graph: FlowGraph,
    pub alignment: BTreeSet<Alignment>,
    pub deltas: BTreeSet<DeltaRecord>,
    pub witnesses: BTreeSet<Witness>,
    pub diagnostics: BTreeSet<Diagnostic>,
    pub unknown_frontiers: BTreeSet<UnknownFrontier>,
    pub capabilities: CapabilitySet,
    pub summaries_used: BTreeSet<ModelVersion>,
    pub flow_extent: FlowExtent,
    pub path_endings: BTreeSet<PathEnding>,
    pub completeness: Completeness,
    pub region_completeness: BTreeMap<String, Coverage>,
    pub stats: AnalysisStats,
}

impl VariableFlowReport {
    pub fn validate(&self) -> Result<(), SchemaError> {
        if self.schema_version != REPORT_SCHEMA_VERSION {
            return Err(SchemaError::UnsupportedVersion {
                found: self.schema_version,
                supported: REPORT_SCHEMA_VERSION,
            });
        }
        self.query.request.selected_binding.declaration.validate()?;
        if let Some(counterpart) = &self.query.request.counterpart {
            counterpart.declaration.validate()?;
        }
        for graph in [&self.before_graph, &self.after_graph] {
            for node in &graph.nodes {
                node.span.validate()?;
            }
        }
        for delta in &self.deltas {
            if let Some(span) = &delta.before_span {
                span.validate()?;
            }
            if let Some(span) = &delta.after_span {
                span.validate()?;
            }
        }
        for diagnostic in &self.diagnostics {
            if let Some(span) = &diagnostic.span {
                span.validate()?;
            }
        }
        for ending in &self.path_endings {
            ending.location.validate()?;
        }
        Ok(())
    }

    /// Serialize with stable bytes. Ordered collections are part of the schema.
    pub fn to_canonical_json(&self) -> Result<Vec<u8>, SchemaError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|error| SchemaError::Serialization(error.to_string()))
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, SchemaError> {
        let report: Self = serde_json::from_slice(bytes)
            .map_err(|error| SchemaError::Serialization(error.to_string()))?;
        report.validate()?;
        Ok(report)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    UnsupportedVersion { found: u32, supported: u32 },
    InvalidSpan { path: String, reason: String },
    Serialization(String),
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "unsupported report schema {found}; supported schema is {supported}"
            ),
            Self::InvalidSpan { path, reason } => {
                write!(f, "invalid source span for {path:?}: {reason}")
            }
            Self::Serialization(reason) => write!(f, "invalid report encoding: {reason}"),
        }
    }
}

impl std::error::Error for SchemaError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputError {
    InvalidSelector(String),
    SnapshotMismatch(String),
    CapabilityMismatch(String),
    InvalidDiff(String),
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSelector(reason) => write!(f, "invalid binding selector: {reason}"),
            Self::SnapshotMismatch(reason) => write!(f, "snapshot mismatch: {reason}"),
            Self::CapabilityMismatch(reason) => write!(f, "capability mismatch: {reason}"),
            Self::InvalidDiff(reason) => write!(f, "invalid repository diff: {reason}"),
        }
    }
}

impl std::error::Error for InputError {}
