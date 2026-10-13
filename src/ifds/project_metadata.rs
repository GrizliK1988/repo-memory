//! Snapshot-scoped TypeScript metadata. This module never changes solver transfers.

use super::{
    Diagnostic, DiagnosticCode, NodeId, Operation, ProcedureId, ProcedureIr, SnapshotHandle,
    SnapshotId, SnapshotProvider, SourceSpan,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

mod analysis;
mod node;
pub use analysis::*;
pub use node::NodeMetadataProvider;

pub const METADATA_PROTOCOL_VERSION: u32 = 1;
pub const METADATA_COMPILER_VERSION: &str = "5.9.3";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataFile {
    pub content_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectOptions {
    /// Repository-relative config path. None requests unambiguous discovery.
    pub tsconfig: Option<String>,
    /// Virtual paths (including node_modules paths) to explicitly supplied configs
    /// and declaration-only files. They cannot replace snapshot files.
    #[serde(default)]
    pub supplied_files: BTreeMap<String, MetadataFile>,
    #[serde(default)]
    pub excluded_paths: BTreeSet<String>,
    /// Declared runtime alias mappings, e.g. #/* -> src/*. No config code runs.
    #[serde(default)]
    pub runtime_aliases: BTreeMap<String, String>,
    /// web or node; selected explicitly, independently for each snapshot.
    pub platform: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataRequest {
    pub protocol_version: u32,
    pub compiler_version: String,
    pub snapshot: SnapshotId,
    pub files: BTreeMap<String, MetadataFile>,
    pub selected_paths: BTreeSet<String>,
    pub options: ProjectOptions,
}

impl MetadataRequest {
    pub fn from_snapshot<P: SnapshotProvider>(
        provider: &P,
        handle: &SnapshotHandle,
        selected_paths: BTreeSet<String>,
        options: ProjectOptions,
    ) -> Result<Self, MetadataError> {
        if provider
            .resolve(handle)
            .map_err(|e| MetadataError::invalid(handle.id.clone(), e.to_string()))?
            != handle.id
        {
            return Err(MetadataError::invalid(
                handle.id.clone(),
                "snapshot identity mismatch",
            ));
        }
        let mut files = BTreeMap::new();
        for (path, content_id) in provider
            .files(handle)
            .map_err(|e| MetadataError::invalid(handle.id.clone(), e.to_string()))?
        {
            // Binary assets cannot contribute TS declarations or config resolution.
            if !is_metadata_path(&path) {
                continue;
            }
            let bytes = provider
                .read_file(handle, &path)
                .map_err(|e| MetadataError::invalid(handle.id.clone(), e.to_string()))?;
            let text = String::from_utf8(bytes)
                .map_err(|e| MetadataError::invalid(handle.id.clone(), format!("{path}: {e}")))?;
            files.insert(path, MetadataFile { content_id, text });
        }
        let request = Self {
            protocol_version: METADATA_PROTOCOL_VERSION,
            compiler_version: METADATA_COMPILER_VERSION.into(),
            snapshot: handle.id.clone(),
            files,
            selected_paths,
            options,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> Result<(), MetadataError> {
        if self.protocol_version != METADATA_PROTOCOL_VERSION
            || self.compiler_version != METADATA_COMPILER_VERSION
        {
            return Err(MetadataError::new(
                "unsupported_version",
                self.snapshot.clone(),
                "unsupported metadata protocol or compiler version",
            ));
        }
        if !matches!(self.options.platform.as_str(), "web" | "node") {
            return Err(MetadataError::invalid(
                self.snapshot.clone(),
                "platform must be web or node",
            ));
        }
        for path in self
            .files
            .keys()
            .chain(self.options.supplied_files.keys())
            .chain(self.selected_paths.iter())
            .chain(self.options.excluded_paths.iter())
            .chain(self.options.tsconfig.iter())
        {
            validate_virtual_path(path)
                .map_err(|message| MetadataError::invalid(self.snapshot.clone(), message))?;
        }
        for (pattern, target) in &self.options.runtime_aliases {
            if pattern.matches('*').count() > 1 || target.matches('*').count() > 1 {
                return Err(MetadataError::invalid(
                    self.snapshot.clone(),
                    "alias mappings support at most one wildcard",
                ));
            }
            validate_virtual_path(target)
                .map_err(|message| MetadataError::invalid(self.snapshot.clone(), message))?;
        }
        if self
            .options
            .supplied_files
            .keys()
            .any(|path| self.files.contains_key(path))
        {
            return Err(MetadataError::invalid(
                self.snapshot.clone(),
                "supplied files cannot replace snapshot files",
            ));
        }
        // Executable dependency code is outside the K017 external-package contract.
        if self.options.supplied_files.keys().any(|path| {
            !path.ends_with(".json")
                && !path.ends_with(".d.ts")
                && !path.ends_with(".d.mts")
                && !path.ends_with(".d.cts")
        }) {
            return Err(MetadataError::invalid(
                self.snapshot.clone(),
                "supplied files must be JSON configs or declaration files",
            ));
        }
        if self
            .selected_paths
            .iter()
            .any(|path| !self.files.contains_key(path))
        {
            return Err(MetadataError::invalid(
                self.snapshot.clone(),
                "selected_paths must name existing snapshot files (empty is allowed for an absent comparison side)",
            ));
        }
        if self.snapshot.revision.is_empty()
            || self.snapshot.content_id.is_empty()
            || self
                .files
                .values()
                .chain(self.options.supplied_files.values())
                .any(|file| file.content_id.is_empty())
        {
            return Err(MetadataError::invalid(
                self.snapshot.clone(),
                "snapshot and file content identities must be nonempty",
            ));
        }
        if let Some(path) = &self.options.tsconfig
            && self.file(path).is_none()
        {
            let mut error = MetadataError::new(
                "missing_tsconfig",
                self.snapshot.clone(),
                "Provide the selected tsconfig",
            );
            error.specifier = Some(path.clone());
            return Err(error);
        }
        Ok(())
    }

    pub fn file(&self, path: &str) -> Option<&MetadataFile> {
        self.files
            .get(path)
            .or_else(|| self.options.supplied_files.get(path))
    }
}

fn is_metadata_path(path: &str) -> bool {
    [
        ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs", ".json",
    ]
    .iter()
    .any(|suffix| path.ends_with(suffix))
}

fn validate_virtual_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path.contains('\0')
        || path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err(format!(
            "invalid repository-relative virtual path: {path:?}"
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataError {
    pub code: String,
    pub snapshot: Box<SnapshotId>,
    pub message: String,
    pub requested_by: Option<String>,
    pub specifier: Option<String>,
    #[serde(default)]
    pub candidates: Box<[String]>,
}

impl MetadataError {
    pub fn new(code: &str, snapshot: SnapshotId, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            snapshot: Box::new(snapshot),
            message: message.into(),
            requested_by: None,
            specifier: None,
            candidates: Box::default(),
        }
    }
    fn invalid(snapshot: SnapshotId, message: impl Into<String>) -> Self {
        Self::new("invalid_request", snapshot, message)
    }
    pub fn is_configuration_error(&self) -> bool {
        matches!(
            self.code.as_str(),
            "missing_tsconfig"
                | "missing_base_config"
                | "ambiguous_tsconfig"
                | "invalid_config"
                | "invalid_request"
                | "unsupported_version"
        )
    }
}
impl fmt::Display for MetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for MetadataError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclarationMetadata {
    /// Deterministic within this snapshot; source identity, never a TS symbol ID.
    pub id: String,
    pub name: String,
    pub kind: String,
    pub span: SourceSpan,
    pub has_body: bool,
    pub implementation_span: Option<SourceSpan>,
    pub declaration_only: bool,
    pub excluded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionStep {
    pub span: SourceSpan,
    pub kind: String,
    pub specifier: Option<String>,
    pub resolved_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStatus {
    ResolvedBody,
    DeclarationOnly,
    ExternalPackage,
    MissingDependency,
    ExcludedBody,
    Unresolved,
    RuntimeMismatch,
    UnsupportedDispatch,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignatureMetadata {
    pub parameters: Vec<String>,
    pub return_type: String,
    pub declaration: Option<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallMetadata {
    pub span: SourceSpan,
    pub callee_span: SourceSpan,
    pub callee: String,
    pub status: ResolutionStatus,
    pub target: Option<String>,
    pub candidates: Vec<String>,
    pub external_package: Option<String>,
    pub signature: Option<SignatureMetadata>,
    pub chain: Vec<ResolutionStep>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportMetadata {
    pub span: SourceSpan,
    pub specifier: String,
    pub resolved_path: Option<String>,
    pub status: ResolutionStatus,
    pub type_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMetadata {
    pub protocol_version: u32,
    pub compiler_version: String,
    pub snapshot: SnapshotId,
    pub tsconfig: String,
    pub configuration_files: BTreeMap<String, MetadataFile>,
    pub platform: String,
    pub selected_paths: BTreeSet<String>,
    pub excluded_paths: BTreeSet<String>,
    pub runtime_aliases: BTreeMap<String, String>,
    pub compiler_options: serde_json::Value,
    pub declarations: Vec<DeclarationMetadata>,
    pub imports: Vec<ImportMetadata>,
    pub calls: Vec<CallMetadata>,
    pub diagnostics: Vec<Diagnostic>,
}

pub trait MetadataProvider {
    fn metadata(&self, request: &MetadataRequest) -> Result<ProjectMetadata, MetadataError>;
}

impl ProjectMetadata {
    pub fn validate(&self, request: &MetadataRequest) -> Result<(), MetadataError> {
        request.validate()?;
        let invalid =
            |message| MetadataError::new("invalid_response", request.snapshot.clone(), message);
        if self.protocol_version != METADATA_PROTOCOL_VERSION
            || self.compiler_version != METADATA_COMPILER_VERSION
            || self.snapshot != request.snapshot
            || self.platform != request.options.platform
            || self.selected_paths != request.selected_paths
            || self.excluded_paths != request.options.excluded_paths
            || self.runtime_aliases != request.options.runtime_aliases
        {
            return Err(invalid("provider version, snapshot or platform mismatch"));
        }
        if request
            .options
            .tsconfig
            .as_ref()
            .is_some_and(|path| path != &self.tsconfig)
            || !self.configuration_files.contains_key(&self.tsconfig)
        {
            return Err(invalid("provider config differs from requested config"));
        }
        for (path, file) in &self.configuration_files {
            if request.file(path) != Some(file) {
                return Err(invalid("provider configuration content mismatch"));
            }
        }
        let mut ids = BTreeSet::new();
        for declaration in &self.declarations {
            if !ids.insert(&declaration.id)
                || declaration.id != declaration_identity(&declaration.span, &declaration.kind)
                || (declaration.declaration_only && declaration.has_body)
            {
                return Err(invalid("invalid declaration identity or body claim"));
            }
            validate_response_span(request, &declaration.span)?;
            if declaration.has_body != declaration.implementation_span.is_some() {
                return Err(invalid("declaration implementation/body mismatch"));
            }
            if let Some(span) = &declaration.implementation_span {
                validate_response_span(request, span)?;
                if span.path != declaration.span.path
                    || span.byte_start < declaration.span.byte_start
                    || span.byte_end > declaration.span.byte_end
                {
                    return Err(invalid("implementation is outside its declaration"));
                }
            }
        }
        let mut call_spans = BTreeSet::new();
        for call in &self.calls {
            validate_response_span(request, &call.span)?;
            validate_response_span(request, &call.callee_span)?;
            let source = &request
                .file(&call.callee_span.path)
                .expect("validated span source")
                .text;
            if source.get(call.callee_span.byte_start as usize..call.callee_span.byte_end as usize)
                != Some(call.callee.as_str())
            {
                return Err(invalid("callee text does not match the snapshot"));
            }
            if !call_spans.insert((&call.span.path, call.span.byte_start, call.span.byte_end))
                || call.callee_span.path != call.span.path
                || call.callee_span.byte_start < call.span.byte_start
                || call.callee_span.byte_end > call.span.byte_end
            {
                return Err(invalid("duplicate call or invalid callee containment"));
            }
            if call
                .candidates
                .iter()
                .chain(call.target.iter())
                .any(|id| !ids.contains(id))
            {
                return Err(invalid("call refers to an unknown declaration"));
            }
            if call
                .target
                .as_ref()
                .is_some_and(|id| !call.candidates.contains(id))
            {
                return Err(invalid("target is not a declared candidate"));
            }
            if call.status == ResolutionStatus::ResolvedBody {
                let target = call
                    .target
                    .as_ref()
                    .and_then(|id| self.declarations.iter().find(|d| &d.id == id));
                if target.is_none_or(|d| {
                    !d.has_body
                        || d.declaration_only
                        || d.excluded
                        || d.span.path.contains("node_modules/")
                }) {
                    return Err(invalid("resolved call has no supported source body"));
                }
            } else if call.target.is_some() {
                return Err(invalid(
                    "unresolved call cannot claim a unique runtime target",
                ));
            }
            if let Some(signature) = &call.signature
                && let Some(span) = &signature.declaration
            {
                validate_response_span(request, span)?;
            }
            for step in &call.chain {
                validate_response_span(request, &step.span)?;
                if let Some(path) = &step.resolved_path {
                    validate_virtual_path(path).map_err(|message| {
                        MetadataError::new("invalid_response", request.snapshot.clone(), message)
                    })?;
                }
            }
        }
        for import in &self.imports {
            validate_response_span(request, &import.span)?;
            if let Some(path) = &import.resolved_path {
                validate_virtual_path(path).map_err(|message| {
                    MetadataError::new("invalid_response", request.snapshot.clone(), message)
                })?;
            }
        }
        for diagnostic in &self.diagnostics {
            if diagnostic.snapshot.as_ref() != Some(&request.snapshot) {
                return Err(invalid("diagnostic snapshot mismatch"));
            }
            if let Some(span) = &diagnostic.span {
                validate_response_span(request, span)?;
            }
        }
        Ok(())
    }

    pub fn render_text(&self, verbose: bool) -> String {
        let mut lines = vec![format!(
            "TypeScript {} · {} · {}",
            self.compiler_version, self.platform, self.tsconfig
        )];
        for call in &self.calls {
            let target = call
                .target
                .as_ref()
                .and_then(|id| self.declarations.iter().find(|d| &d.id == id))
                .map(|d| format!("{} ({}:{})", d.name, d.span.path, d.span.start_line))
                .unwrap_or_else(|| "unknown target".into());
            lines.push(format!(
                "{}:{} {} → {} [{:?}] — {}",
                call.span.path, call.span.start_line, call.callee, target, call.status, call.reason
            ));
            if verbose {
                for step in &call.chain {
                    lines.push(format!(
                        "  {}:{} {} {}",
                        step.span.path,
                        step.span.start_line,
                        step.kind,
                        step.specifier.as_deref().unwrap_or("")
                    ));
                }
            }
        }
        lines.join("\n")
    }
}

fn validate_response_span(
    request: &MetadataRequest,
    span: &SourceSpan,
) -> Result<(), MetadataError> {
    let invalid = || {
        MetadataError::new(
            "invalid_response",
            request.snapshot.clone(),
            format!("invalid provider span: {span:?}"),
        )
    };
    let file = request.file(&span.path).ok_or_else(invalid)?;
    span.validate().map_err(|_| invalid())?;
    let start = usize::try_from(span.byte_start).map_err(|_| invalid())?;
    let end = usize::try_from(span.byte_end).map_err(|_| invalid())?;
    if file.text.get(start..end).is_none() {
        return Err(invalid());
    }
    let start_line = file.text[..start].bytes().filter(|b| *b == b'\n').count() as u32 + 1;
    let end_position = if end > start {
        file.text[..end]
            .char_indices()
            .next_back()
            .map_or(start, |(i, _)| i)
    } else {
        start
    };
    let end_line = file.text[..end_position]
        .bytes()
        .filter(|b| *b == b'\n')
        .count() as u32
        + 1;
    if span.start_line != start_line || span.end_line != end_line {
        return Err(invalid());
    }
    Ok(())
}

pub fn declaration_identity(span: &SourceSpan, kind: &str) -> String {
    format!("{}:{}:{}:{kind}", span.path, span.byte_start, span.byte_end)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundCallMetadata {
    pub node: NodeId,
    pub call_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcedureMetadataBinding {
    pub procedure: ProcedureId,
    pub path: String,
    pub calls: Vec<BoundCallMetadata>,
}

/// Match the actual call operation, not reads/writes that share an expression span.
/// Path remains part of the key: existing procedure IDs are file-local.
pub fn bind_procedure_metadata(
    procedure: &ProcedureIr,
    metadata: &ProjectMetadata,
) -> Result<ProcedureMetadataBinding, MetadataError> {
    if procedure.id.snapshot != metadata.snapshot {
        return Err(MetadataError::new(
            "invalid_response",
            metadata.snapshot.clone(),
            "IR/metadata snapshot mismatch",
        ));
    }
    let path = procedure
        .nodes
        .values()
        .find_map(|node| node.span.as_ref().map(|s| s.path.clone()))
        .unwrap_or_default();
    let mut calls = Vec::new();
    for (call_index, call) in metadata.calls.iter().enumerate() {
        let matches: Vec<_> = procedure
            .nodes
            .values()
            .filter(|node| {
                node.span.as_ref() == Some(&call.span)
                    && match &node.operation {
                        Operation::Call { .. } => true,
                        Operation::UnknownEffect { description, .. } => matches!(
                            description.as_str(),
                            "unsupported TypeScript construct \"call_expression\""
                                | "unsupported TypeScript construct \"new_expression\""
                        ),
                        _ => false,
                    }
            })
            .collect();
        if matches.len() > 1 {
            return Err(MetadataError::new(
                "invalid_response",
                metadata.snapshot.clone(),
                "ambiguous call operation mapping",
            ));
        }
        if let Some(node) = matches.first() {
            calls.push(BoundCallMetadata {
                node: node.id.clone(),
                call_index,
            });
        }
    }
    Ok(ProcedureMetadataBinding {
        procedure: procedure.id.clone(),
        path,
        calls,
    })
}

pub fn metadata_failure_diagnostic(error: &MetadataError) -> Diagnostic {
    Diagnostic {
        code: DiagnosticCode::TypeResolutionUnavailable,
        message: error.to_string(),
        snapshot: Some((*error.snapshot).clone()),
        frontier: None,
        span: None,
        affected_flows: BTreeSet::from(["project_metadata".into()]),
    }
}

#[cfg(test)]
mod tests;
