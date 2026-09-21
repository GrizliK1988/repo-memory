//! Offline fixture loading, reviewed expectations, and independent tiny-IR oracles.

use crate::ifds::ir::{Operation, ProcedureIr};
use crate::ifds::model::{
    AnalysisLimits, BindingSelector, CapabilitySet, Completeness, DiagnosticCode, Direction,
    EntryAssumption, Fact, FlowDelta, FlowExtent, FlowGraph, MembershipChange, ModelVersion,
    NodeId, PathEndingKind, Place, RelationKind, SnapshotSide, Source, SourceSpan,
    VariableFlowReport,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const FIXTURE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixturePartition {
    Development,
    HeldOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureSnapshot {
    pub revision: String,
    pub content_id: String,
    pub directory: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureSnapshots {
    pub before: FixtureSnapshot,
    pub after: FixtureSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureProvenance {
    pub repository: String,
    pub revision: Option<String>,
    pub pull_request: Option<String>,
    pub license: Option<String>,
    pub upstream_paths: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureManifest {
    pub schema_version: u32,
    pub case_id: String,
    pub category_id: String,
    pub promoted: bool,
    pub partition: FixturePartition,
    pub project_configuration: BTreeMap<String, String>,
    pub snapshots: FixtureSnapshots,
    pub change_file: String,
    pub selector: BindingSelector,
    pub counterpart: Option<BindingSelector>,
    pub entry_assumptions: BTreeSet<EntryAssumption>,
    pub capabilities: CapabilitySet,
    pub model_versions: BTreeSet<ModelVersion>,
    pub limits: AnalysisLimits,
    pub expected_file: String,
    pub provenance: Option<FixtureProvenance>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LabelLocation {
    Node { span: SourceSpan },
    Binding { span: SourceSpan },
}

impl LabelLocation {
    fn span(&self) -> &SourceSpan {
        match self {
            Self::Node { span } | Self::Binding { span } => span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogicalLabel {
    pub before: Option<LabelLocation>,
    pub after: Option<LabelLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedRelation {
    pub source: String,
    pub consumer: String,
    pub relation: RelationKind,
    pub projection: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedSidedRelation {
    pub side: SnapshotSide,
    #[serde(flatten)]
    pub relation: ExpectedRelation,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExpectedDelta {
    NodeAdded {
        node: String,
    },
    NodeRemoved {
        node: String,
    },
    SliceMembershipChanged {
        node: String,
        change: MembershipChange,
    },
    OperationChanged {
        node: String,
    },
    WriteAdded {
        node: String,
    },
    WriteRemoved {
        node: String,
    },
    FlowAdded {
        relation: ExpectedRelation,
    },
    FlowRemoved {
        relation: ExpectedRelation,
    },
    FlowConditionChanged {
        relation: ExpectedRelation,
        before: String,
        after: String,
    },
    ValueSourceChanged {
        consumer: String,
        projection: String,
        before_sources: BTreeSet<String>,
        after_sources: BTreeSet<String>,
    },
    AnalysisUnknown {
        subject: String,
        side: Option<SnapshotSide>,
        diagnostic: DiagnosticCode,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedEnding {
    pub side: SnapshotSide,
    pub kind: PathEndingKind,
    pub location: SourceSpan,
    pub condition: Option<String>,
    pub diagnostic: Option<DiagnosticCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WitnessAssertion {
    pub label: String,
    pub ordered_nodes: Vec<String>,
    pub required_backedges: BTreeSet<(String, String)>,
    pub required_summary_expansions: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedUnknownFrontier {
    pub node: String,
    pub diagnostic: DiagnosticCode,
    pub direction: Direction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureExpectations {
    pub schema_version: u32,
    pub labels: BTreeMap<String, LogicalLabel>,
    #[serde(default)]
    pub required: BTreeSet<ExpectedDelta>,
    #[serde(default)]
    pub forbidden: BTreeSet<ExpectedDelta>,
    #[serde(default)]
    pub retained_relations: BTreeSet<ExpectedRelation>,
    #[serde(default)]
    pub required_relations: BTreeSet<ExpectedSidedRelation>,
    #[serde(default)]
    pub forbidden_relations: BTreeSet<ExpectedSidedRelation>,
    #[serde(default)]
    pub required_endings: BTreeSet<ExpectedEnding>,
    #[serde(default)]
    pub witness_assertions: BTreeSet<WitnessAssertion>,
    #[serde(default)]
    pub required_unknown_frontiers: BTreeSet<ExpectedUnknownFrontier>,
    #[serde(default)]
    pub summaries_used: BTreeSet<ModelVersion>,
    pub coverage: FlowExtent,
    pub completeness: Completeness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedFixture {
    pub root: PathBuf,
    pub manifest: FixtureManifest,
    pub expected: FixtureExpectations,
}

#[derive(Debug)]
pub enum FixtureError {
    Io { path: PathBuf, message: String },
    Json { path: PathBuf, message: String },
    MissingField(&'static str),
    MissingFile(PathBuf),
    Invalid(String),
    Mismatch(Vec<String>),
}

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => {
                write!(f, "fixture I/O error at {}: {message}", path.display())
            }
            Self::Json { path, message } => {
                write!(f, "invalid fixture JSON at {}: {message}", path.display())
            }
            Self::MissingField(field) => write!(f, "fixture is missing required field `{field}`"),
            Self::MissingFile(path) => {
                write!(f, "fixture is missing required file `{}`", path.display())
            }
            Self::Invalid(message) => write!(f, "invalid fixture: {message}"),
            Self::Mismatch(messages) => {
                write!(f, "fixture report mismatch: {}", messages.join("; "))
            }
        }
    }
}

impl std::error::Error for FixtureError {}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, FixtureError> {
    let bytes = fs::read(path).map_err(|error| FixtureError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    serde_json::from_slice(&bytes).map_err(|error| FixtureError::Json {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn safe_child(root: &Path, relative: &str, field: &'static str) -> Result<PathBuf, FixtureError> {
    if relative.is_empty() {
        return Err(FixtureError::MissingField(field));
    }
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(FixtureError::Invalid(format!(
            "{field} must be a relative path without traversal"
        )));
    }
    Ok(root.join(path))
}

pub fn discover_fixtures(root: &Path) -> Result<Vec<PathBuf>, FixtureError> {
    let entries = fs::read_dir(root).map_err(|error| FixtureError::Io {
        path: root.to_path_buf(),
        message: error.to_string(),
    })?;
    let mut fixtures = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| FixtureError::Io {
            path: root.to_path_buf(),
            message: error.to_string(),
        })?;
        if entry
            .file_type()
            .map_err(|error| FixtureError::Io {
                path: entry.path(),
                message: error.to_string(),
            })?
            .is_dir()
            && entry.path().join("manifest.json").is_file()
        {
            fixtures.push(entry.path());
        }
    }
    fixtures.sort();
    if fixtures.is_empty() {
        return Err(FixtureError::Invalid(format!(
            "no fixtures discovered under {}",
            root.display()
        )));
    }
    Ok(fixtures)
}

pub fn load_fixture(root: &Path) -> Result<LoadedFixture, FixtureError> {
    let manifest_path = root.join("manifest.json");
    if !manifest_path.is_file() {
        return Err(FixtureError::MissingFile(manifest_path));
    }
    let manifest: FixtureManifest = read_json(&manifest_path)?;
    validate_manifest(root, &manifest)?;
    let expected_path = safe_child(root, &manifest.expected_file, "expected_file")?;
    let expected: FixtureExpectations = read_json(&expected_path)?;
    validate_expectations(&expected)?;
    validate_fixture_spans(root, &manifest, &expected)?;
    Ok(LoadedFixture {
        root: root.to_path_buf(),
        manifest,
        expected,
    })
}

fn require_file(root: &Path, relative: &str, field: &'static str) -> Result<(), FixtureError> {
    let path = safe_child(root, relative, field)?;
    if !path.is_file() {
        return Err(FixtureError::MissingFile(path));
    }
    Ok(())
}

fn require_dir(root: &Path, relative: &str, field: &'static str) -> Result<(), FixtureError> {
    let path = safe_child(root, relative, field)?;
    if !path.is_dir() {
        return Err(FixtureError::MissingFile(path));
    }
    Ok(())
}

pub fn validate_manifest(root: &Path, manifest: &FixtureManifest) -> Result<(), FixtureError> {
    if manifest.schema_version != FIXTURE_SCHEMA_VERSION {
        return Err(FixtureError::Invalid(format!(
            "unsupported manifest schema {}",
            manifest.schema_version
        )));
    }
    if manifest.case_id.is_empty() {
        return Err(FixtureError::MissingField("case_id"));
    }
    if manifest.category_id.is_empty() {
        return Err(FixtureError::MissingField("category_id"));
    }
    if manifest.selector.declaration.path.is_empty() {
        return Err(FixtureError::MissingField("selector"));
    }
    manifest
        .selector
        .declaration
        .validate()
        .map_err(|error| FixtureError::Invalid(error.to_string()))?;
    if let Some(counterpart) = &manifest.counterpart {
        counterpart
            .declaration
            .validate()
            .map_err(|error| FixtureError::Invalid(error.to_string()))?;
    }
    if manifest.model_versions.is_empty() {
        return Err(FixtureError::MissingField("model_versions"));
    }
    if manifest.expected_file.is_empty() {
        return Err(FixtureError::MissingField("expected_file"));
    }
    require_dir(
        root,
        &manifest.snapshots.before.directory,
        "snapshots.before.directory",
    )?;
    require_dir(
        root,
        &manifest.snapshots.after.directory,
        "snapshots.after.directory",
    )?;
    require_file(root, &manifest.change_file, "change_file")?;
    require_file(root, &manifest.expected_file, "expected_file")?;
    require_file(root, "README.md", "README.md")?;
    Ok(())
}

fn validate_fixture_spans(
    root: &Path,
    manifest: &FixtureManifest,
    expected: &FixtureExpectations,
) -> Result<(), FixtureError> {
    if !expected.summaries_used.is_subset(&manifest.model_versions) {
        return Err(FixtureError::Invalid(
            "expected summary assumptions are not declared by the manifest".into(),
        ));
    }
    let snapshot_for = |side| match side {
        SnapshotSide::Before => &manifest.snapshots.before,
        SnapshotSide::After => &manifest.snapshots.after,
    };
    for selector in std::iter::once(&manifest.selector).chain(manifest.counterpart.iter()) {
        let snapshot = snapshot_for(selector.snapshot.side);
        if selector.snapshot.revision != snapshot.revision
            || selector.snapshot.content_id != snapshot.content_id
        {
            return Err(FixtureError::Invalid(
                "selector snapshot identity does not match the manifest".into(),
            ));
        }
        validate_span_in_snapshot(root, snapshot, &selector.declaration)?;
    }
    for label in expected.labels.values() {
        for (side, location) in [
            (SnapshotSide::Before, label.before.as_ref()),
            (SnapshotSide::After, label.after.as_ref()),
        ] {
            if let Some(location) = location {
                validate_span_in_snapshot(root, snapshot_for(side), location.span())?;
            }
        }
    }
    for ending in &expected.required_endings {
        validate_span_in_snapshot(root, snapshot_for(ending.side), &ending.location)?;
    }
    Ok(())
}

fn validate_span_in_snapshot(
    root: &Path,
    snapshot: &FixtureSnapshot,
    span: &SourceSpan,
) -> Result<(), FixtureError> {
    let directory = safe_child(root, &snapshot.directory, "snapshot directory")?;
    let path = safe_child(&directory, &span.path, "source span path")?;
    let bytes = fs::read(&path).map_err(|error| FixtureError::Io {
        path: path.clone(),
        message: error.to_string(),
    })?;
    let source = std::str::from_utf8(&bytes).map_err(|error| {
        FixtureError::Invalid(format!("{} is not UTF-8: {error}", path.display()))
    })?;
    let start = usize::try_from(span.byte_start)
        .map_err(|_| FixtureError::Invalid("span start does not fit this platform".into()))?;
    let end = usize::try_from(span.byte_end)
        .map_err(|_| FixtureError::Invalid("span end does not fit this platform".into()))?;
    if end > source.len() || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
        return Err(FixtureError::Invalid(format!(
            "span is outside UTF-8 boundaries in {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_expectations(expected: &FixtureExpectations) -> Result<(), FixtureError> {
    if expected.schema_version != FIXTURE_SCHEMA_VERSION {
        return Err(FixtureError::Invalid(format!(
            "unsupported expected schema {}",
            expected.schema_version
        )));
    }
    for (name, label) in &expected.labels {
        if name.is_empty() || (label.before.is_none() && label.after.is_none()) {
            return Err(FixtureError::Invalid(
                "logical labels need a name and at least one location".into(),
            ));
        }
        for location in [label.before.as_ref(), label.after.as_ref()]
            .into_iter()
            .flatten()
        {
            location
                .span()
                .validate()
                .map_err(|error| FixtureError::Invalid(error.to_string()))?;
        }
    }
    let require_label = |label: &str| {
        if expected.labels.contains_key(label) {
            Ok(())
        } else {
            Err(FixtureError::Invalid(format!(
                "expectation references undeclared logical label `{label}`"
            )))
        }
    };
    let validate_relation = |relation: &ExpectedRelation| {
        require_label(&relation.source)?;
        require_label(&relation.consumer)
    };
    for relation in &expected.retained_relations {
        validate_relation(relation)?;
    }
    for relation in expected
        .required_relations
        .iter()
        .chain(&expected.forbidden_relations)
    {
        validate_relation(&relation.relation)?;
    }
    for delta in expected.required.iter().chain(&expected.forbidden) {
        match delta {
            ExpectedDelta::NodeAdded { node }
            | ExpectedDelta::NodeRemoved { node }
            | ExpectedDelta::SliceMembershipChanged { node, .. }
            | ExpectedDelta::OperationChanged { node }
            | ExpectedDelta::WriteAdded { node }
            | ExpectedDelta::WriteRemoved { node } => require_label(node)?,
            ExpectedDelta::FlowAdded { relation }
            | ExpectedDelta::FlowRemoved { relation }
            | ExpectedDelta::FlowConditionChanged { relation, .. } => validate_relation(relation)?,
            ExpectedDelta::ValueSourceChanged {
                consumer,
                before_sources,
                after_sources,
                ..
            } => {
                require_label(consumer)?;
                for source in before_sources.iter().chain(after_sources) {
                    require_label(source)?;
                }
            }
            ExpectedDelta::AnalysisUnknown { .. } => {}
        }
    }
    for witness in &expected.witness_assertions {
        for node in &witness.ordered_nodes {
            require_label(node)?;
        }
        for (source, target) in &witness.required_backedges {
            require_label(source)?;
            require_label(target)?;
        }
    }
    for frontier in &expected.required_unknown_frontiers {
        require_label(&frontier.node)?;
    }
    Ok(())
}

struct Labels<'a> {
    expected: &'a FixtureExpectations,
    report: &'a VariableFlowReport,
}

impl Labels<'_> {
    fn node(&self, id: &NodeId) -> Result<String, FixtureError> {
        let graph = match id.snapshot.side {
            SnapshotSide::Before => &self.report.before_graph,
            SnapshotSide::After => &self.report.after_graph,
        };
        let node = graph
            .nodes
            .iter()
            .find(|node| &node.id == id)
            .ok_or_else(|| {
                FixtureError::Invalid(format!("report references missing node {id:?}"))
            })?;
        self.by_span(id.snapshot.side, &node.span)
    }

    fn logical(&self, id: crate::ifds::model::LogicalNodeId) -> Result<String, FixtureError> {
        let alignment = self
            .report
            .alignment
            .iter()
            .find(|item| item.logical == id)
            .ok_or_else(|| {
                FixtureError::Invalid(format!("report references unaligned logical node {id:?}"))
            })?;
        if let Some(node) = alignment.before.as_ref().or(alignment.after.as_ref()) {
            self.node(node)
        } else {
            Err(FixtureError::Invalid(format!(
                "logical node {id:?} has no runtime node"
            )))
        }
    }

    fn by_span(&self, side: SnapshotSide, span: &SourceSpan) -> Result<String, FixtureError> {
        reviewed_label(self.expected, side, span)
    }
}

fn reviewed_label(
    expected: &FixtureExpectations,
    side: SnapshotSide,
    span: &SourceSpan,
) -> Result<String, FixtureError> {
    let mut matches = expected
        .labels
        .iter()
        .filter(|(_, label)| {
            let location = match side {
                SnapshotSide::Before => &label.before,
                SnapshotSide::After => &label.after,
            };
            location
                .as_ref()
                .is_some_and(|location| location.span() == span)
        })
        .map(|(name, _)| name.clone());
    let first = if let Some(first) = matches.next() {
        first
    } else {
        let containing: Vec<_> = expected
            .labels
            .iter()
            .filter_map(|(name, label)| {
                let location = match side {
                    SnapshotSide::Before => &label.before,
                    SnapshotSide::After => &label.after,
                };
                let outer = location.as_ref()?.span();
                (outer.path == span.path
                    && outer.byte_start <= span.byte_start
                    && span.byte_end <= outer.byte_end)
                    .then_some((outer.byte_end - outer.byte_start, name.clone()))
            })
            .collect();
        let shortest = containing
            .iter()
            .map(|(size, _)| *size)
            .min()
            .ok_or_else(|| {
                FixtureError::Invalid(format!("no reviewed label for {side:?} span {span:?}"))
            })?;
        let mut nearest = containing.into_iter().filter(|(size, _)| *size == shortest);
        let (_, first) = nearest.next().expect("a shortest containing label exists");
        if nearest.next().is_some() {
            return Err(FixtureError::Invalid(format!(
                "ambiguous reviewed containing label for {side:?} span {span:?}"
            )));
        }
        return Ok(first);
    };
    if matches.next().is_some() {
        return Err(FixtureError::Invalid(format!(
            "ambiguous reviewed label for {side:?} span {span:?}"
        )));
    }
    Ok(first)
}

fn normalize_relation(
    labels: &Labels<'_>,
    source: crate::ifds::model::LogicalNodeId,
    target: crate::ifds::model::LogicalNodeId,
    relation: &RelationKind,
    projection: &Option<String>,
) -> Result<ExpectedRelation, FixtureError> {
    Ok(ExpectedRelation {
        source: labels.logical(source)?,
        consumer: labels.logical(target)?,
        relation: relation.clone(),
        projection: projection.clone(),
    })
}

fn normalize_delta(labels: &Labels<'_>, delta: &FlowDelta) -> Result<ExpectedDelta, FixtureError> {
    Ok(match delta {
        FlowDelta::NodeAdded { after } => ExpectedDelta::NodeAdded {
            node: labels.node(after)?,
        },
        FlowDelta::NodeRemoved { before } => ExpectedDelta::NodeRemoved {
            node: labels.node(before)?,
        },
        FlowDelta::SliceMembershipChanged { logical, change } => {
            ExpectedDelta::SliceMembershipChanged {
                node: labels.logical(*logical)?,
                change: change.clone(),
            }
        }
        FlowDelta::OperationChanged { logical, .. } => ExpectedDelta::OperationChanged {
            node: labels.logical(*logical)?,
        },
        FlowDelta::WriteAdded { after } => ExpectedDelta::WriteAdded {
            node: labels.node(after)?,
        },
        FlowDelta::WriteRemoved { before } => ExpectedDelta::WriteRemoved {
            node: labels.node(before)?,
        },
        FlowDelta::FlowAdded {
            source,
            target,
            relation,
            projection,
        } => ExpectedDelta::FlowAdded {
            relation: normalize_relation(labels, *source, *target, relation, projection)?,
        },
        FlowDelta::FlowRemoved {
            source,
            target,
            relation,
            projection,
        } => ExpectedDelta::FlowRemoved {
            relation: normalize_relation(labels, *source, *target, relation, projection)?,
        },
        FlowDelta::FlowConditionChanged {
            source,
            target,
            relation,
            projection,
            before,
            after,
        } => ExpectedDelta::FlowConditionChanged {
            relation: normalize_relation(labels, *source, *target, relation, projection)?,
            before: before.clone(),
            after: after.clone(),
        },
        FlowDelta::ValueSourceChanged {
            consumer,
            projection,
            before_sources,
            after_sources,
            ..
        } => ExpectedDelta::ValueSourceChanged {
            consumer: labels.logical(*consumer)?,
            projection: projection.clone(),
            before_sources: before_sources
                .iter()
                .map(|id| labels.logical(*id))
                .collect::<Result<_, _>>()?,
            after_sources: after_sources
                .iter()
                .map(|id| labels.logical(*id))
                .collect::<Result<_, _>>()?,
        },
        FlowDelta::AnalysisUnknown {
            subject,
            side,
            diagnostic,
        } => ExpectedDelta::AnalysisUnknown {
            subject: subject.clone(),
            side: *side,
            diagnostic: diagnostic.clone(),
        },
    })
}

fn graph_relations(
    labels: &Labels<'_>,
    graph: &FlowGraph,
) -> Result<BTreeSet<ExpectedRelation>, FixtureError> {
    graph
        .edges
        .iter()
        .map(|edge| {
            Ok(ExpectedRelation {
                source: labels.node(&edge.source)?,
                consumer: labels.node(&edge.target)?,
                relation: edge.relation.clone(),
                projection: edge.projection.clone(),
            })
        })
        .collect()
}

fn compare_reviewed_sets(
    expected: &FixtureExpectations,
    actual: &BTreeSet<ExpectedDelta>,
    before: &BTreeSet<ExpectedRelation>,
    after: &BTreeSet<ExpectedRelation>,
    endings: &BTreeSet<ExpectedEnding>,
) -> Vec<String> {
    let mut failures = Vec::new();
    for required in expected.required.difference(actual) {
        failures.push(format!("missing required assertion {required:?}"));
    }
    for forbidden in expected.forbidden.intersection(actual) {
        failures.push(format!("reported forbidden assertion {forbidden:?}"));
    }
    for expected_relation in &expected.required_relations {
        let graph = match expected_relation.side {
            SnapshotSide::Before => before,
            SnapshotSide::After => after,
        };
        if !graph.contains(&expected_relation.relation) {
            failures.push(format!("missing required relation {expected_relation:?}"));
        }
    }
    for forbidden_relation in &expected.forbidden_relations {
        let graph = match forbidden_relation.side {
            SnapshotSide::Before => before,
            SnapshotSide::After => after,
        };
        if graph.contains(&forbidden_relation.relation) {
            failures.push(format!(
                "reported forbidden relation {forbidden_relation:?}"
            ));
        }
    }
    for retained in &expected.retained_relations {
        if !before.contains(retained) || !after.contains(retained) {
            failures.push(format!(
                "retained relation is not present on both sides {retained:?}"
            ));
        }
        if actual.contains(&ExpectedDelta::FlowAdded {
            relation: retained.clone(),
        }) || actual.contains(&ExpectedDelta::FlowRemoved {
            relation: retained.clone(),
        }) {
            failures.push(format!("retained relation was misclassified {retained:?}"));
        }
    }
    for ending in expected.required_endings.difference(endings) {
        failures.push(format!("missing required ending {ending:?}"));
    }
    failures
}

pub fn compare_report(
    expected: &FixtureExpectations,
    report: &VariableFlowReport,
) -> Result<(), FixtureError> {
    let labels = Labels { expected, report };
    let actual: BTreeSet<_> = report
        .deltas
        .iter()
        .map(|record| normalize_delta(&labels, &record.delta))
        .collect::<Result<_, _>>()?;
    let before = graph_relations(&labels, &report.before_graph)?;
    let after = graph_relations(&labels, &report.after_graph)?;
    let endings = report
        .path_endings
        .iter()
        .map(|ending| ExpectedEnding {
            side: ending.origin_side(),
            kind: ending.kind.clone(),
            location: ending.location.clone(),
            condition: ending.condition.clone(),
            diagnostic: ending.diagnostic.clone(),
        })
        .collect();
    let mut failures = compare_reviewed_sets(expected, &actual, &before, &after, &endings);
    for assertion in &expected.witness_assertions {
        let mut matched = false;
        for witness in &report.witnesses {
            let nodes = witness
                .nodes
                .iter()
                .map(|node| labels.node(node))
                .collect::<Result<Vec<_>, _>>()?;
            let backedges = witness
                .backedges
                .iter()
                .map(|(source, target)| Ok((labels.node(source)?, labels.node(target)?)))
                .collect::<Result<BTreeSet<_>, FixtureError>>()?;
            if nodes == assertion.ordered_nodes
                && assertion.required_backedges.is_subset(&backedges)
                && assertion
                    .required_summary_expansions
                    .is_subset(&witness.summary_expansions)
            {
                matched = true;
                break;
            }
        }
        if !matched {
            failures.push(format!("missing required witness `{}`", assertion.label));
        }
    }
    let actual_frontiers = report
        .unknown_frontiers
        .iter()
        .map(|frontier| {
            Ok(ExpectedUnknownFrontier {
                node: labels.node(&frontier.node)?,
                diagnostic: frontier.diagnostic.clone(),
                direction: frontier.affected_direction,
            })
        })
        .collect::<Result<BTreeSet<_>, FixtureError>>()?;
    for frontier in expected
        .required_unknown_frontiers
        .difference(&actual_frontiers)
    {
        failures.push(format!("missing required unknown frontier {frontier:?}"));
    }
    if report.summaries_used != expected.summaries_used {
        failures.push("summary assumptions differ".into());
    }
    if report.flow_extent != expected.coverage {
        failures.push("directional coverage differs".into());
    }
    if report.completeness != expected.completeness {
        failures.push("completeness differs".into());
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(FixtureError::Mismatch(failures))
    }
}

trait PathEndingSide {
    fn origin_side(&self) -> SnapshotSide;
}
impl PathEndingSide for crate::ifds::model::PathEnding {
    fn origin_side(&self) -> SnapshotSide {
        match &self.origin {
            Source::Write(id) | Source::ParameterEntry(id) => id.snapshot.side,
            Source::FunctionInput(id) => id.snapshot.side,
            Source::ModeledExternal { .. } => self.carrier.snapshot_side(),
        }
    }
}

trait PlaceSide {
    fn snapshot_side(&self) -> SnapshotSide;
}
impl PlaceSide for Place {
    fn snapshot_side(&self) -> SnapshotSide {
        match self {
            Place::Binding(id) => id.snapshot.side,
            Place::Temporary(id) => id.snapshot.side,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceEvaluation {
    pub facts_at: BTreeMap<NodeId, BTreeSet<Fact>>,
    pub complete: bool,
    pub processed_states: u64,
}

fn facts_for_place(
    facts: &BTreeSet<Fact>,
    place: &Place,
) -> (BTreeSet<crate::ifds::model::DefinitionId>, BTreeSet<Source>) {
    let mut writes = BTreeSet::new();
    let mut origins = BTreeSet::new();
    for fact in facts {
        match fact {
            Fact::LastWrite {
                place: candidate,
                write,
            } if candidate == place => {
                writes.insert(write.clone());
            }
            Fact::Origin {
                place: candidate,
                source,
            } if candidate == place => {
                origins.insert(source.clone());
            }
            _ => {}
        }
    }
    (writes, origins)
}

fn kill_place(facts: &mut BTreeSet<Fact>, place: &Place) {
    facts.retain(|fact| !matches!(fact,
        Fact::LastWrite { place: candidate, .. } | Fact::Origin { place: candidate, .. } if candidate == place));
}

fn reference_transfer(
    operation: &Operation,
    incoming: &BTreeSet<Fact>,
) -> Result<BTreeSet<Fact>, FixtureError> {
    let mut outgoing = incoming.clone();
    match operation {
        Operation::Read { source, result } => {
            let (_, origins) = facts_for_place(incoming, source);
            kill_place(&mut outgoing, result);
            outgoing.extend(origins.into_iter().map(|source| Fact::Origin {
                place: result.clone(),
                source,
            }));
        }
        Operation::Write {
            target,
            sources,
            definition,
        } => {
            let origins: BTreeSet<_> = sources
                .iter()
                .flat_map(|source| facts_for_place(incoming, source).1)
                .collect();
            kill_place(&mut outgoing, target);
            if incoming.contains(&Fact::Zero) {
                outgoing.insert(Fact::LastWrite {
                    place: target.clone(),
                    write: definition.clone(),
                });
                outgoing.insert(Fact::Origin {
                    place: target.clone(),
                    source: Source::Write(definition.clone()),
                });
            }
            outgoing.extend(origins.into_iter().map(|source| Fact::Origin {
                place: target.clone(),
                source,
            }));
        }
        Operation::Compute {
            definition,
            inputs,
            result,
            ..
        } => {
            let origins: BTreeSet<_> = inputs
                .iter()
                .flat_map(|input| facts_for_place(incoming, &input.place).1)
                .collect();
            kill_place(&mut outgoing, result);
            if incoming.contains(&Fact::Zero) {
                outgoing.insert(Fact::LastWrite {
                    place: result.clone(),
                    write: definition.clone(),
                });
                outgoing.insert(Fact::Origin {
                    place: result.clone(),
                    source: Source::Write(definition.clone()),
                });
            }
            outgoing.extend(origins.into_iter().map(|source| Fact::Origin {
                place: result.clone(),
                source,
            }));
        }
        Operation::Literal {
            definition, result, ..
        } => {
            kill_place(&mut outgoing, result);
            if incoming.contains(&Fact::Zero) {
                outgoing.insert(Fact::LastWrite {
                    place: result.clone(),
                    write: definition.clone(),
                });
                outgoing.insert(Fact::Origin {
                    place: result.clone(),
                    source: Source::Write(definition.clone()),
                });
            }
        }
        Operation::Call { .. } | Operation::UnknownEffect { .. } => {
            return Err(FixtureError::Invalid(
                "the tiny reference evaluator cannot prove effects for calls or unknown operations"
                    .into(),
            ));
        }
        Operation::Entry
        | Operation::Branch { .. }
        | Operation::Join
        | Operation::ReturnSite { .. }
        | Operation::Return { .. }
        | Operation::Exit => {}
    }
    Ok(outgoing)
}

/// Evaluates every supplied finite entry state independently to a fixed point.
/// Reaching `max_processed_states` returns the known prefix with `complete = false`.
pub fn evaluate_reference(
    procedure: &ProcedureIr,
    entry_states: &[BTreeSet<Fact>],
    max_processed_states: u64,
) -> Result<ReferenceEvaluation, FixtureError> {
    procedure
        .validate()
        .map_err(|error| FixtureError::Invalid(error.to_string()))?;
    let mut combined: BTreeMap<NodeId, BTreeSet<Fact>> = BTreeMap::new();
    let mut processed = 0;
    let mut complete = true;
    let outgoing: BTreeMap<_, Vec<_>> = procedure
        .nodes
        .keys()
        .map(|node| {
            let targets = procedure
                .edges
                .iter()
                .filter(|edge| &edge.source == node)
                .map(|edge| edge.target.clone())
                .collect();
            (node.clone(), targets)
        })
        .collect();
    for initial in entry_states {
        let mut states = BTreeMap::<NodeId, BTreeSet<Fact>>::new();
        states.insert(procedure.entry.clone(), initial.clone());
        let mut queue = VecDeque::from([procedure.entry.clone()]);
        while let Some(node_id) = queue.pop_front() {
            if processed >= max_processed_states {
                complete = false;
                break;
            }
            processed += 1;
            let incoming = states.get(&node_id).cloned().unwrap_or_default();
            combined
                .entry(node_id.clone())
                .or_default()
                .extend(incoming.clone());
            let node = &procedure.nodes[&node_id];
            let transferred = reference_transfer(&node.operation, &incoming)?;
            for target in &outgoing[&node_id] {
                let first_visit = !states.contains_key(target);
                let target_state = states.entry(target.clone()).or_default();
                let old_len = target_state.len();
                target_state.extend(transferred.clone());
                if first_visit || target_state.len() != old_len {
                    queue.push_back(target.clone());
                }
            }
        }
        if !complete {
            break;
        }
    }
    Ok(ReferenceEvaluation {
        facts_at: combined,
        complete,
        processed_states: processed,
    })
}

pub fn is_distributive<T, F>(domain: &BTreeSet<T>, transfer: F) -> bool
where
    T: Clone + Ord,
    F: Fn(&BTreeSet<T>) -> BTreeSet<T>,
{
    let values: Vec<_> = domain.iter().cloned().collect();
    if values.len() >= usize::BITS as usize {
        return false;
    }
    let subsets: Vec<BTreeSet<T>> = (0usize..(1usize << values.len()))
        .map(|mask| {
            values
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .map(|(_, value)| value.clone())
                .collect()
        })
        .collect();
    subsets.iter().all(|left| {
        subsets.iter().all(|right| {
            let union = left.union(right).cloned().collect();
            transfer(&union) == transfer(left).union(&transfer(right)).cloned().collect()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ifds::ir::{IrEdge, IrNode};
    use crate::ifds::model::{
        Alignment, AnalysisStats, BindingId, DefinitionId, DeltaRecord, EntryPoint, FlowEdge,
        FlowNode, PathEnding, ProcedureId, REPORT_SCHEMA_VERSION, RepositoryDiff, ResolvedQuery,
        SnapshotHandle, SnapshotId, VariableFlowQuery,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_ID: AtomicU64 = AtomicU64::new(0);
    fn snapshot() -> SnapshotId {
        SnapshotId {
            side: SnapshotSide::Before,
            revision: "r".into(),
            content_id: "c".into(),
        }
    }
    fn node(local: u64) -> NodeId {
        NodeId::new(snapshot(), local)
    }
    fn place(local: u64) -> Place {
        Place::Binding(crate::ifds::model::BindingId::new(snapshot(), local))
    }
    fn definition(local: u64) -> DefinitionId {
        DefinitionId::new(snapshot(), local)
    }
    fn temp() -> PathBuf {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "repo-memory-k003-{}-{timestamp}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn tiny_ir() -> ProcedureIr {
        let nodes = [
            (1, Operation::Entry),
            (
                2,
                Operation::Write {
                    target: place(1),
                    sources: BTreeSet::new(),
                    definition: definition(1),
                },
            ),
            (
                3,
                Operation::Read {
                    source: place(1),
                    result: Place::Temporary(node(3)),
                },
            ),
            (
                4,
                Operation::Write {
                    target: place(1),
                    sources: BTreeSet::new(),
                    definition: definition(2),
                },
            ),
            (5, Operation::Exit),
        ]
        .into_iter()
        .map(|(local, operation)| {
            (
                node(local),
                IrNode {
                    id: node(local),
                    operation,
                    span: None,
                },
            )
        })
        .collect();
        let edges = (1..5)
            .map(|local| IrEdge {
                source: node(local),
                target: node(local + 1),
                kind: crate::ifds::ir::EdgeKind::Normal,
                construct: None,
                assumption: None,
            })
            .collect();
        ProcedureIr {
            id: ProcedureId::new(snapshot(), 1),
            parameters: Vec::new(),
            entry: node(1),
            exits: BTreeSet::from([node(5)]),
            nodes,
            edges,
        }
    }

    fn source_span(path: &str, start: u64) -> SourceSpan {
        SourceSpan {
            path: path.into(),
            byte_start: start,
            byte_end: start + 1,
            start_line: 1,
            end_line: 1,
        }
    }

    fn extent() -> FlowExtent {
        let snapshot = crate::ifds::model::SnapshotExtent {
            upstream: crate::ifds::model::Coverage::Complete,
            downstream: crate::ifds::model::Coverage::Complete,
            declared_scope: "fixture".into(),
            source_boundaries: BTreeSet::new(),
            sink_boundaries: BTreeSet::new(),
            value_lifecycle_closed: true,
        };
        FlowExtent {
            before: snapshot.clone(),
            after: snapshot,
        }
    }

    fn expectations() -> FixtureExpectations {
        FixtureExpectations {
            schema_version: 1,
            labels: BTreeMap::new(),
            required: BTreeSet::new(),
            forbidden: BTreeSet::new(),
            retained_relations: BTreeSet::new(),
            required_relations: BTreeSet::new(),
            forbidden_relations: BTreeSet::new(),
            required_endings: BTreeSet::new(),
            witness_assertions: BTreeSet::new(),
            required_unknown_frontiers: BTreeSet::new(),
            summaries_used: BTreeSet::new(),
            coverage: extent(),
            completeness: Completeness::CompleteForQuery,
        }
    }

    fn relation(source: &str, consumer: &str) -> ExpectedRelation {
        ExpectedRelation {
            source: source.into(),
            consumer: consumer.into(),
            relation: RelationKind::Reaches,
            projection: None,
        }
    }

    fn synthetic_report() -> (FixtureExpectations, VariableFlowReport) {
        let snap = |side| SnapshotId {
            side,
            revision: format!("{side:?}"),
            content_id: format!("{side:?}-content"),
        };
        let before = snap(SnapshotSide::Before);
        let after = snap(SnapshotSide::After);
        let bn = |local| NodeId::new(before.clone(), local);
        let an = |local| NodeId::new(after.clone(), local + 100);
        let spans = [
            source_span("main.ts", 1),
            source_span("main.ts", 3),
            source_span("main.ts", 5),
        ];
        let node = |id, span: &SourceSpan| FlowNode {
            id,
            operation: "fixture".into(),
            span: span.clone(),
            enclosing_declaration: Some("fixture".into()),
            fingerprint: "fixture".into(),
        };
        let edge = |source, target| FlowEdge {
            source,
            target,
            relation: RelationKind::Reaches,
            projection: None,
            condition: None,
            evidence: crate::ifds::model::EvidenceKind::Supported,
        };
        let capabilities = CapabilitySet {
            stage: 0,
            capabilities: BTreeSet::from(["fixture_harness".into()]),
            version: "stage-0".into(),
        };
        let limits = AnalysisLimits {
            time_ms: 1,
            memory_bytes: 1,
            processed_path_edges: 100,
            witnesses_per_relation: 2,
            output_nodes: 10,
        };
        let before_binding = BindingId::new(before.clone(), 1);
        let after_binding = BindingId::new(after.clone(), 1);
        let selector = |snapshot| BindingSelector {
            snapshot,
            declaration: spans[0].clone(),
            expected_name: Some("x".into()),
            expected_enclosing_symbol: Some("fixture".into()),
        };
        let report = VariableFlowReport {
            schema_version: REPORT_SCHEMA_VERSION,
            analysis_version: "test".into(),
            snapshots: BTreeSet::from([before.clone(), after.clone()]),
            query: ResolvedQuery {
                request: VariableFlowQuery {
                    before: SnapshotHandle {
                        id: before.clone(),
                        repository_id: "fixture".into(),
                    },
                    after: SnapshotHandle {
                        id: after.clone(),
                        repository_id: "fixture".into(),
                    },
                    diff: RepositoryDiff {
                        before_content_id: before.content_id.clone(),
                        after_content_id: after.content_id.clone(),
                        changes: BTreeSet::new(),
                    },
                    selected_binding: selector(before.clone()),
                    counterpart: Some(selector(after.clone())),
                    entry: EntryPoint::ContainingFunction,
                    capabilities: capabilities.clone(),
                    summaries: BTreeSet::new(),
                    limits,
                },
                before_binding: Some(before_binding),
                after_binding: Some(after_binding.clone()),
            },
            entry_assumptions: BTreeSet::new(),
            before_graph: FlowGraph {
                nodes: spans
                    .iter()
                    .enumerate()
                    .map(|(i, span)| node(bn(i as u64 + 1), span))
                    .collect(),
                edges: BTreeSet::from([edge(bn(1), bn(2)), edge(bn(2), bn(3))]),
                facts: BTreeSet::new(),
            },
            after_graph: FlowGraph {
                nodes: spans
                    .iter()
                    .enumerate()
                    .map(|(i, span)| node(an(i as u64 + 1), span))
                    .collect(),
                edges: BTreeSet::from([edge(an(1), an(2)), edge(an(2), an(3))]),
                facts: BTreeSet::new(),
            },
            alignment: (1..=3)
                .map(|local| Alignment {
                    logical: crate::ifds::model::LogicalNodeId(local),
                    before: Some(bn(local)),
                    after: Some(an(local)),
                    evidence: "reviewed".into(),
                })
                .collect(),
            deltas: BTreeSet::new(),
            human_summary: "No established flow changes within the analyzed scope.".into(),
            witnesses: BTreeSet::new(),
            diagnostics: BTreeSet::new(),
            unknown_frontiers: BTreeSet::new(),
            capabilities,
            summaries_used: BTreeSet::new(),
            flow_extent: extent(),
            path_endings: BTreeSet::from([PathEnding {
                kind: PathEndingKind::ExecutionExit,
                origin: Source::FunctionInput(after_binding.clone()),
                carrier: Place::Binding(after_binding),
                location: spans[2].clone(),
                condition: None,
                context: "fixture".into(),
                witness: crate::ifds::model::WitnessId(1),
                model: None,
                diagnostic: None,
            }]),
            completeness: Completeness::CompleteForQuery,
            region_completeness: BTreeMap::from([
                ("upstream".into(), crate::ifds::model::Coverage::Complete),
                ("downstream".into(), crate::ifds::model::Coverage::Complete),
            ]),
            stats: AnalysisStats {
                elapsed_ms: 0,
                peak_memory_bytes: 0,
                processed_path_edges: 0,
                graph_nodes: 6,
                graph_edges: 4,
                witnesses_emitted: 0,
                witnesses_omitted: 0,
                limits_hit: BTreeSet::new(),
                graph_truncated: false,
                witnesses_truncated: false,
            },
        };
        let mut expected = expectations();
        for (label, span) in ["source", "consumer", "final_consumer"]
            .into_iter()
            .zip(spans)
        {
            expected.labels.insert(
                label.into(),
                LogicalLabel {
                    before: Some(LabelLocation::Node { span: span.clone() }),
                    after: Some(LabelLocation::Node { span }),
                },
            );
        }
        (expected, report)
    }

    #[test]
    fn ifds_k003_manifest_validation() {
        let root = temp();
        let manifest = FixtureManifest {
            schema_version: 1,
            case_id: "case".into(),
            category_id: "writes".into(),
            promoted: true,
            partition: FixturePartition::Development,
            project_configuration: BTreeMap::new(),
            snapshots: FixtureSnapshots {
                before: FixtureSnapshot {
                    revision: "a".into(),
                    content_id: "a".into(),
                    directory: "before".into(),
                },
                after: FixtureSnapshot {
                    revision: "b".into(),
                    content_id: "b".into(),
                    directory: "after".into(),
                },
            },
            change_file: "change.diff".into(),
            selector: BindingSelector {
                snapshot: snapshot(),
                declaration: SourceSpan {
                    path: "x.ts".into(),
                    byte_start: 0,
                    byte_end: 1,
                    start_line: 1,
                    end_line: 1,
                },
                expected_name: Some("x".into()),
                expected_enclosing_symbol: None,
            },
            counterpart: None,
            entry_assumptions: BTreeSet::new(),
            capabilities: CapabilitySet {
                stage: 0,
                capabilities: BTreeSet::new(),
                version: "0".into(),
            },
            model_versions: BTreeSet::new(),
            limits: AnalysisLimits {
                time_ms: 1,
                memory_bytes: 1,
                processed_path_edges: 1,
                witnesses_per_relation: 1,
                output_nodes: 1,
            },
            expected_file: "expected.json".into(),
            provenance: None,
        };
        let mut missing_selector = serde_json::to_value(&manifest).unwrap();
        missing_selector.as_object_mut().unwrap().remove("selector");
        fs::write(
            root.join("manifest.json"),
            serde_json::to_vec(&missing_selector).unwrap(),
        )
        .unwrap();
        let error = load_fixture(&root).unwrap_err().to_string();
        assert!(error.contains("selector"), "{error}");

        let error = validate_manifest(&root, &manifest).unwrap_err().to_string();
        assert!(error.contains("model_versions"), "{error}");
        let mut with_model = manifest;
        with_model.model_versions.insert(ModelVersion {
            name: "none".into(),
            version: "1".into(),
            digest: "reviewed".into(),
        });
        fs::create_dir(root.join("before")).unwrap();
        fs::create_dir(root.join("after")).unwrap();
        fs::write(root.join("change.diff"), b"").unwrap();
        fs::write(root.join("README.md"), b"reviewed").unwrap();
        let error = validate_manifest(&root, &with_model)
            .unwrap_err()
            .to_string();
        assert!(error.contains("expected.json"), "{error}");
    }

    #[test]
    fn ifds_k003_required_forbidden_retained() {
        let (base_expected, base_report) = synthetic_report();
        let flow = relation("source", "consumer");

        let mut required = base_expected.clone();
        required.required.insert(ExpectedDelta::FlowAdded {
            relation: flow.clone(),
        });
        let error = compare_report(&required, &base_report)
            .unwrap_err()
            .to_string();
        assert!(error.contains("missing required"), "{error}");

        let forbidden_delta = ExpectedDelta::FlowRemoved {
            relation: flow.clone(),
        };
        let mut forbidden = base_expected.clone();
        forbidden.forbidden.insert(forbidden_delta.clone());
        let mut forbidden_report = base_report.clone();
        forbidden_report.deltas.insert(DeltaRecord {
            delta: FlowDelta::FlowRemoved {
                source: crate::ifds::model::LogicalNodeId(1),
                target: crate::ifds::model::LogicalNodeId(2),
                relation: RelationKind::Reaches,
                projection: None,
            },
            groups: BTreeSet::new(),
            before_span: None,
            after_span: None,
            condition: None,
        });
        let error = compare_report(&forbidden, &forbidden_report)
            .unwrap_err()
            .to_string();
        assert!(error.contains("forbidden"), "{error}");

        let mut retained = base_expected;
        retained.retained_relations.insert(flow.clone());
        let mut retained_report = base_report;
        retained_report.deltas.insert(DeltaRecord {
            delta: FlowDelta::FlowAdded {
                source: crate::ifds::model::LogicalNodeId(1),
                target: crate::ifds::model::LogicalNodeId(2),
                relation: RelationKind::Reaches,
                projection: None,
            },
            groups: BTreeSet::new(),
            before_span: None,
            after_span: None,
            condition: None,
        });
        let error = compare_report(&retained, &retained_report)
            .unwrap_err()
            .to_string();
        assert!(error.contains("misclassified"), "{error}");
    }

    #[test]
    fn ifds_k003_full_chain_required() {
        let (mut expected, mut report) = synthetic_report();
        let first = relation("source", "consumer");
        let later = relation("consumer", "final_consumer");
        let ending = ExpectedEnding {
            side: SnapshotSide::After,
            kind: PathEndingKind::ExecutionExit,
            location: source_span("main.ts", 5),
            condition: None,
            diagnostic: None,
        };
        expected.required_relations.extend([
            ExpectedSidedRelation {
                side: SnapshotSide::After,
                relation: first.clone(),
            },
            ExpectedSidedRelation {
                side: SnapshotSide::After,
                relation: later,
            },
        ]);
        expected.required_endings.insert(ending);
        report
            .after_graph
            .edges
            .retain(|edge| edge.target.local != 103);
        report.path_endings.clear();
        let error = compare_report(&expected, &report).unwrap_err().to_string();
        assert!(error.contains("final_consumer"), "{error}");
        assert!(error.contains("ending"), "{error}");
    }

    #[test]
    fn ifds_k003_logical_label_normalization() {
        let (mut report_expected, report) = synthetic_report();
        report_expected
            .retained_relations
            .insert(relation("source", "consumer"));
        compare_report(&report_expected, &report).unwrap();
        report_expected.retained_relations.clear();
        report_expected
            .retained_relations
            .insert(relation("consumer", "source"));
        assert!(compare_report(&report_expected, &report).is_err());

        let before_source = source_span("before/main.ts", 1);
        let after_source = source_span("after/main.ts", 10);
        let before_consumer = source_span("before/main.ts", 3);
        let after_consumer = source_span("after/main.ts", 30);
        let mut expected = expectations();
        expected.labels.insert(
            "source".into(),
            LogicalLabel {
                before: Some(LabelLocation::Node {
                    span: before_source.clone(),
                }),
                after: Some(LabelLocation::Node {
                    span: after_source.clone(),
                }),
            },
        );
        expected.labels.insert(
            "consumer".into(),
            LogicalLabel {
                before: Some(LabelLocation::Node {
                    span: before_consumer.clone(),
                }),
                after: Some(LabelLocation::Node {
                    span: after_consumer.clone(),
                }),
            },
        );
        assert_eq!(
            reviewed_label(&expected, SnapshotSide::Before, &before_source).unwrap(),
            reviewed_label(&expected, SnapshotSide::After, &after_source).unwrap()
        );
        let normalized = relation(
            &reviewed_label(&expected, SnapshotSide::Before, &before_source).unwrap(),
            &reviewed_label(&expected, SnapshotSide::Before, &before_consumer).unwrap(),
        );
        let swapped = relation(
            &reviewed_label(&expected, SnapshotSide::After, &after_consumer).unwrap(),
            &reviewed_label(&expected, SnapshotSide::After, &after_source).unwrap(),
        );
        assert_ne!(normalized, swapped);
    }

    #[test]
    fn ifds_k003_tiny_reference_evaluator() {
        let result = evaluate_reference(&tiny_ir(), &[BTreeSet::from([Fact::Zero])], 100).unwrap();
        assert!(result.complete);
        let at_read_successor = &result.facts_at[&node(4)];
        assert!(!at_read_successor.contains(&Fact::LastWrite {
            place: Place::Temporary(node(3)),
            write: definition(1)
        }));
        assert!(at_read_successor.contains(&Fact::Origin {
            place: Place::Temporary(node(3)),
            source: Source::Write(definition(1))
        }));
        let at_exit = &result.facts_at[&node(5)];
        assert!(at_exit.contains(&Fact::LastWrite {
            place: place(1),
            write: definition(2)
        }));
        assert!(!at_exit.contains(&Fact::LastWrite {
            place: place(1),
            write: definition(1)
        }));
    }

    #[test]
    fn ifds_k003_distributivity_harness() {
        let domain = BTreeSet::from([1, 2]);
        assert!(is_distributive(&domain, |facts| facts.clone()));
        assert!(!is_distributive(&domain, |facts| {
            if facts.contains(&1) && facts.contains(&2) {
                BTreeSet::from([1])
            } else {
                BTreeSet::new()
            }
        }));
    }

    #[test]
    fn ifds_k003_bounded_oracle_is_honest() {
        let exact = evaluate_reference(&tiny_ir(), &[BTreeSet::from([Fact::Zero])], 100).unwrap();
        assert!(exact.complete);
        let bounded = evaluate_reference(&tiny_ir(), &[BTreeSet::from([Fact::Zero])], 2).unwrap();
        assert!(!bounded.complete);
        assert!(
            !bounded.facts_at.contains_key(&node(5)),
            "an incomplete run must not invent absence evidence at an unvisited exit"
        );
    }
}
