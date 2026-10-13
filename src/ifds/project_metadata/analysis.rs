use super::*;
use crate::ifds::adapters::typescript::{
    index_bindings, index_functions, lower_function_procedure, lower_program_procedure,
};
use crate::ifds::{
    AnalysisEnvironment, FunctionResultQuery, FunctionResultReport, VariableFlowQuery,
    VariableFlowReport,
};

/// An additive envelope preserves the existing binding/function report contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectAnalysisReport<T> {
    pub schema_version: u32,
    pub analysis: T,
    pub before: SnapshotMetadataReport,
    pub after: SnapshotMetadataReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcedureDeclarationBinding {
    pub declaration: String,
    pub path: String,
    pub procedure: ProcedureId,
    pub function_span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotMetadataReport {
    pub snapshot: SnapshotId,
    pub metadata: Option<ProjectMetadata>,
    pub procedure_declarations: Vec<ProcedureDeclarationBinding>,
    pub procedure_calls: Vec<ProcedureMetadataBinding>,
    pub unmapped_calls: Vec<usize>,
    pub diagnostics: Vec<Diagnostic>,
}

impl SnapshotMetadataReport {
    pub fn render_text(&self, verbose: bool) -> String {
        let Some(metadata) = &self.metadata else {
            return self
                .diagnostics
                .iter()
                .map(|d| d.message.as_str())
                .collect::<Vec<_>>()
                .join("\n");
        };
        let mut text = metadata.render_text(verbose);
        if !self.unmapped_calls.is_empty() {
            text.push_str(&format!(
                "\nNo corresponding scalar IR operation for {} call expressions.",
                self.unmapped_calls.len()
            ));
        }
        text
    }
}

/// Bind selected files and their direct targets. Unsupported lowering never
/// upgrades metadata into a supported argument/return flow.
pub fn collect_project_metadata<M: MetadataProvider>(
    request: &MetadataRequest,
    provider: &M,
) -> Result<SnapshotMetadataReport, MetadataError> {
    request.validate()?;
    let metadata = match provider.metadata(request) {
        Ok(metadata) => {
            metadata.validate(request)?;
            metadata
        }
        Err(error) if *error.snapshot != request.snapshot => {
            return Err(MetadataError::new(
                "invalid_response",
                request.snapshot.clone(),
                "provider error belongs to a different snapshot",
            ));
        }
        Err(error) if !error.is_configuration_error() && error.code != "invalid_response" => {
            return Ok(SnapshotMetadataReport {
                snapshot: request.snapshot.clone(),
                metadata: None,
                procedure_declarations: Vec::new(),
                procedure_calls: Vec::new(),
                unmapped_calls: Vec::new(),
                diagnostics: vec![metadata_failure_diagnostic(&error)],
            });
        }
        Err(error) => return Err(error),
    };
    let mut paths = request.selected_paths.clone();
    for call in &metadata.calls {
        if !request.selected_paths.contains(&call.span.path) {
            continue;
        }
        if let Some(target) = &call.target
            && let Some(declaration) = metadata.declarations.iter().find(|d| &d.id == target)
        {
            paths.insert(declaration.span.path.clone());
        }
    }
    let mut result = SnapshotMetadataReport {
        snapshot: request.snapshot.clone(),
        diagnostics: metadata.diagnostics.clone(),
        metadata: None,
        procedure_declarations: Vec::new(),
        procedure_calls: Vec::new(),
        unmapped_calls: Vec::new(),
    };
    for path in paths {
        let Some(file) = request.files.get(&path) else {
            continue;
        };
        if request
            .options
            .excluded_paths
            .iter()
            .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
        {
            continue;
        }
        let (functions, index) = match index_functions(&path, &file.text).and_then(|functions| {
            index_bindings(request.snapshot.clone(), &path, &file.text)
                .map(|index| (functions, index))
        }) {
            Ok(value) => value,
            Err(error) => {
                result.diagnostics.push(Diagnostic {
                    code: DiagnosticCode::UnsupportedSyntax,
                    message: error.to_string(),
                    snapshot: Some(request.snapshot.clone()),
                    frontier: None,
                    span: None,
                    affected_flows: BTreeSet::from([format!("project_metadata:{path}")]),
                });
                continue;
            }
        };
        let (program, _) = lower_program_procedure(&file.text, &index).map_err(|e| {
            MetadataError::new(
                "ir_mapping_failure",
                request.snapshot.clone(),
                e.to_string(),
            )
        })?;
        result
            .procedure_calls
            .push(bind_procedure_metadata(&program, &metadata)?);
        for function in functions {
            let (procedure, _) = lower_function_procedure(&file.text, &index, &function.span)
                .map_err(|e| {
                    MetadataError::new(
                        "ir_mapping_failure",
                        request.snapshot.clone(),
                        e.to_string(),
                    )
                })?;
            // TS declarations own export modifiers; Tree-sitter functions do not.
            // A variable declaration owns its arrow/function initializer.
            let declaration = metadata
                .declarations
                .iter()
                .filter(|d| {
                    d.has_body
                        && !d.declaration_only
                        && !d.excluded
                        && d.span.path == path
                        && d.implementation_span.as_ref().is_some_and(|span| {
                            span.byte_start <= function.span.byte_start
                                && span.byte_end == function.span.byte_end
                        })
                })
                .min_by_key(|d| d.span.byte_end - d.span.byte_start);
            if let Some(declaration) = declaration {
                result
                    .procedure_declarations
                    .push(ProcedureDeclarationBinding {
                        declaration: declaration.id.clone(),
                        path: path.clone(),
                        procedure: procedure.id.clone(),
                        function_span: function.span.clone(),
                    });
            }
            result
                .procedure_calls
                .push(bind_procedure_metadata(&procedure, &metadata)?);
        }
    }
    let mapped: BTreeSet<_> = result
        .procedure_calls
        .iter()
        .flat_map(|procedure| procedure.calls.iter().map(|call| call.call_index))
        .collect();
    for (index, call) in metadata.calls.iter().enumerate() {
        if request.selected_paths.contains(&call.span.path) && !mapped.contains(&index) {
            result.unmapped_calls.push(index);
            result.diagnostics.push(Diagnostic {
                code: DiagnosticCode::UnresolvedCall,
                message: format!(
                    "{}: metadata is available, but no corresponding call operation was emitted in the scalar IR",
                    call.callee
                ),
                snapshot: Some(request.snapshot.clone()),
                frontier: None,
                span: Some(call.span.clone()),
                affected_flows: BTreeSet::from(["metadata_ir_mapping".into()]),
            });
        }
    }
    result.metadata = Some(metadata);
    Ok(result)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPairOptions {
    pub before: ProjectOptions,
    pub after: ProjectOptions,
}

fn paired_paths(
    selected: &SourceSpan,
    selected_side: crate::ifds::SnapshotSide,
    counterpart: Option<&SourceSpan>,
    diff: &crate::ifds::RepositoryDiff,
) -> (String, String) {
    let other = counterpart
        .map(|span| span.path.clone())
        .unwrap_or_else(|| {
            diff.changes
                .iter()
                .find_map(|change| match selected_side {
                    crate::ifds::SnapshotSide::Before
                        if change.before_path.as_deref() == Some(&selected.path) =>
                    {
                        change.after_path.clone()
                    }
                    crate::ifds::SnapshotSide::After
                        if change.after_path.as_deref() == Some(&selected.path) =>
                    {
                        change.before_path.clone()
                    }
                    _ => None,
                })
                .unwrap_or_else(|| selected.path.clone())
        });
    match selected_side {
        crate::ifds::SnapshotSide::Before => (selected.path.clone(), other),
        crate::ifds::SnapshotSide::After => (other, selected.path.clone()),
    }
}

fn side_request<P: SnapshotProvider>(
    snapshots: &P,
    handle: &SnapshotHandle,
    path: String,
    options: ProjectOptions,
) -> Result<MetadataRequest, MetadataError> {
    let mut request = MetadataRequest::from_snapshot(snapshots, handle, BTreeSet::new(), options)?;
    if request.files.contains_key(&path) {
        request.selected_paths.insert(path);
    }
    request.validate()?;
    Ok(request)
}

#[derive(Debug)]
pub enum ProjectAnalysisError {
    Metadata(MetadataError),
    Analysis(String),
}
impl fmt::Display for ProjectAnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Metadata(error) => write!(f, "{error}"),
            Self::Analysis(error) => write!(f, "{error}"),
        }
    }
}
impl std::error::Error for ProjectAnalysisError {}
impl From<MetadataError> for ProjectAnalysisError {
    fn from(error: MetadataError) -> Self {
        Self::Metadata(error)
    }
}

pub fn analyze_variable_flow_with_metadata<P: SnapshotProvider, M: MetadataProvider>(
    query: VariableFlowQuery,
    snapshots: &P,
    environments: (&AnalysisEnvironment, &AnalysisEnvironment),
    options: ProjectPairOptions,
    metadata_provider: &M,
) -> Result<ProjectAnalysisReport<VariableFlowReport>, ProjectAnalysisError> {
    let (before_path, after_path) = paired_paths(
        &query.selected_binding.declaration,
        query.selected_binding.snapshot.side,
        query.counterpart.as_ref().map(|s| &s.declaration),
        &query.diff,
    );
    let before_request = side_request(snapshots, &query.before, before_path, options.before)?;
    let after_request = side_request(snapshots, &query.after, after_path, options.after)?;
    let before = collect_project_metadata(&before_request, metadata_provider)?;
    let after = collect_project_metadata(&after_request, metadata_provider)?;
    let after_path = after_request
        .selected_paths
        .iter()
        .next()
        .cloned()
        .unwrap_or_default();
    let before_path = before_request
        .selected_paths
        .iter()
        .next()
        .cloned()
        .unwrap_or_default();
    let before_project = before
        .metadata
        .as_ref()
        .map(|metadata| {
            super::super::interprocedural::ProjectProgram::from_metadata(
                &before_request,
                metadata,
                &before_path,
            )
        })
        .transpose()
        .map_err(ProjectAnalysisError::Analysis)?;
    let after_project = after
        .metadata
        .as_ref()
        .map(|metadata| {
            super::super::interprocedural::ProjectProgram::from_metadata(
                &after_request,
                metadata,
                &after_path,
            )
        })
        .transpose()
        .map_err(ProjectAnalysisError::Analysis)?;
    let (analysis, _) = super::super::analysis::analyze_variable_flow_reports_with_projects(
        query,
        snapshots,
        environments.0,
        environments.1,
        before_project.as_ref(),
        after_project.as_ref(),
    )
    .map_err(|e| ProjectAnalysisError::Analysis(e.to_string()))?;
    Ok(ProjectAnalysisReport {
        schema_version: 1,
        analysis,
        before,
        after,
    })
}

pub fn analyze_function_result_with_metadata<P: SnapshotProvider, M: MetadataProvider>(
    query: FunctionResultQuery,
    snapshots: &P,
    environments: (&AnalysisEnvironment, &AnalysisEnvironment),
    options: ProjectPairOptions,
    metadata_provider: &M,
) -> Result<ProjectAnalysisReport<FunctionResultReport>, ProjectAnalysisError> {
    let (before_path, after_path) = paired_paths(
        &query.selected_function.declaration,
        query.selected_function.snapshot.side,
        query.counterpart.as_ref().map(|s| &s.declaration),
        &query.diff,
    );
    let before_request = side_request(snapshots, &query.before, before_path, options.before)?;
    let after_request = side_request(snapshots, &query.after, after_path, options.after)?;
    let before = collect_project_metadata(&before_request, metadata_provider)?;
    let after = collect_project_metadata(&after_request, metadata_provider)?;
    let before_path = before_request
        .selected_paths
        .iter()
        .next()
        .cloned()
        .unwrap_or_default();
    let after_path = after_request
        .selected_paths
        .iter()
        .next()
        .cloned()
        .unwrap_or_default();
    let before_project = before
        .metadata
        .as_ref()
        .map(|metadata| {
            super::super::interprocedural::ProjectProgram::from_metadata(
                &before_request,
                metadata,
                &before_path,
            )
        })
        .transpose()
        .map_err(ProjectAnalysisError::Analysis)?;
    let after_project = after
        .metadata
        .as_ref()
        .map(|metadata| {
            super::super::interprocedural::ProjectProgram::from_metadata(
                &after_request,
                metadata,
                &after_path,
            )
        })
        .transpose()
        .map_err(ProjectAnalysisError::Analysis)?;
    let analysis = super::super::function_results::analyze_function_result_with_projects(
        query,
        snapshots,
        environments.0,
        environments.1,
        before_project.as_ref(),
        after_project.as_ref(),
    )
    .map_err(|e| ProjectAnalysisError::Analysis(e.to_string()))?;
    let analysis = super::super::function_result_report::assemble_function_result_report(analysis);
    Ok(ProjectAnalysisReport {
        schema_version: 1,
        analysis,
        before,
        after,
    })
}
