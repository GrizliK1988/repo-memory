use super::*;

fn request() -> MetadataRequest {
    let snapshot = SnapshotId {
        side: crate::ifds::SnapshotSide::Before,
        revision: "fixture-revision".into(),
        content_id: "fixture-tree".into(),
    };
    let files = [
        ("tsconfig.json", r##"{"compilerOptions":{"module":"esnext","moduleResolution":"bundler","target":"es2022","paths":{"#/*":["./src/*"]}},"include":["src"]}"##),
        ("src/main.ts", "import { helper as run } from '#/barrel'; export function main(input: string) { return run(input); }"),
        ("src/barrel.ts", "export { helper } from './helper';"),
        ("src/helper.ts", "export function helper(value: string) { return value; }"),
    ].into_iter().map(|(path, text)| (path.into(), MetadataFile { content_id: format!("content:{path}"), text: text.into() })).collect();
    MetadataRequest {
        protocol_version: METADATA_PROTOCOL_VERSION,
        compiler_version: METADATA_COMPILER_VERSION.into(),
        snapshot,
        files,
        selected_paths: BTreeSet::from(["src/main.ts".into()]),
        options: ProjectOptions {
            tsconfig: Some("tsconfig.json".into()),
            platform: "web".into(),
            ..ProjectOptions::default()
        },
    }
}

fn real(request: &MetadataRequest) -> ProjectMetadata {
    NodeMetadataProvider::default().metadata(request).unwrap()
}
fn target<'a>(metadata: &'a ProjectMetadata, call: &CallMetadata) -> &'a DeclarationMetadata {
    metadata
        .declarations
        .iter()
        .find(|d| Some(&d.id) == call.target.as_ref())
        .unwrap()
}

#[test]
fn ifds_k017_protocol_versions() {
    let mut request = request();
    let metadata = real(&request);
    let json = serde_json::to_vec(&metadata).unwrap();
    let decoded: ProjectMetadata = serde_json::from_slice(&json).unwrap();
    decoded.validate(&request).unwrap();
    request.protocol_version += 1;
    assert_eq!(request.validate().unwrap_err().code, "unsupported_version");
    request.protocol_version = METADATA_PROTOCOL_VERSION;
    request.compiler_version = "wrong".into();
    assert_eq!(request.validate().unwrap_err().code, "unsupported_version");
    request.compiler_version = METADATA_COMPILER_VERSION.into();
    let mut wrong = metadata;
    wrong.snapshot.content_id = "wrong".into();
    assert_eq!(
        wrong.validate(&request).unwrap_err().code,
        "invalid_response"
    );
}

#[test]
fn ifds_k017_aliases_and_reexports() {
    let request = request();
    let metadata = real(&request);
    let call = metadata.calls.iter().find(|c| c.callee == "run").unwrap();
    assert_eq!(call.status, ResolutionStatus::ResolvedBody);
    assert_eq!(target(&metadata, call).span.path, "src/helper.ts");
    assert!(
        call.chain
            .iter()
            .any(|s| s.kind == "import" && s.specifier.as_deref() == Some("#/barrel"))
    );
    assert!(
        call.chain
            .iter()
            .any(|s| s.kind == "reexport" && s.specifier.as_deref() == Some("./helper"))
    );
    let report = collect_project_metadata(&request, &NodeMetadataProvider::default()).unwrap();
    assert_eq!(
        report
            .procedure_calls
            .iter()
            .map(|p| p.calls.len())
            .sum::<usize>(),
        1
    );
    assert!(
        report
            .procedure_declarations
            .iter()
            .any(|p| p.path == "src/helper.ts")
    );
    let canonical = serde_json::to_vec(&metadata).unwrap();
    assert_eq!(canonical, serde_json::to_vec(&real(&request)).unwrap());
}

#[test]
fn ifds_k017_local_shadowing() {
    let mut request = request();
    request.files.get_mut("src/main.ts").unwrap().text = "import { helper as run } from '#/barrel'; export function main(input: string) { const run = (value: string) => value; return run(input); }".into();
    let metadata = real(&request);
    let call = metadata.calls.iter().find(|c| c.callee == "run").unwrap();
    assert_eq!(call.status, ResolutionStatus::ResolvedBody);
    assert_eq!(target(&metadata, call).span.path, "src/main.ts");
    assert!(call.chain.iter().all(|s| s.kind != "import"));
}

#[test]
fn ifds_k017_config_dependent_target() {
    let mut request = request();
    request.files.get_mut("src/main.ts").unwrap().text = "import { helper } from '#/helper'; export function main(input: string) { return helper(input); }".into();
    request.files.insert(
        "src/helper.web.ts".into(),
        MetadataFile {
            content_id: "web".into(),
            text: "export function helper(value: string) { return value; }".into(),
        },
    );
    request.files.insert(
        "tsconfig.web.json".into(),
        MetadataFile {
            content_id: "web-config".into(),
            text:
                r#"{"extends":"./tsconfig.json","compilerOptions":{"moduleSuffixes":[".web",""]}}"#
                    .into(),
        },
    );
    let before = real(&request);
    assert_eq!(target(&before, &before.calls[0]).span.path, "src/helper.ts");
    request.options.tsconfig = Some("tsconfig.web.json".into());
    let after = real(&request);
    assert_eq!(
        target(&after, &after.calls[0]).span.path,
        "src/helper.web.ts"
    );
    request
        .options
        .runtime_aliases
        .insert("#/*".into(), "wrong/*".into());
    let mismatched = real(&request);
    assert_eq!(
        mismatched.calls[0].status,
        ResolutionStatus::RuntimeMismatch
    );
    assert!(mismatched.calls[0].target.is_none());
}

#[test]
fn ifds_k017_declarations_not_effects() {
    let mut request = request();
    request.files.remove("src/helper.ts");
    request.files.insert(
        "src/helper.d.ts".into(),
        MetadataFile {
            content_id: "types".into(),
            text: "export declare function helper(value: string): string;".into(),
        },
    );
    let metadata = real(&request);
    assert_eq!(metadata.calls[0].status, ResolutionStatus::DeclarationOnly);
    assert!(metadata.calls[0].target.is_none());
    assert_eq!(
        metadata.calls[0].signature.as_ref().unwrap().return_type,
        "string"
    );
    assert!(
        metadata
            .declarations
            .iter()
            .filter(|d| d.declaration_only)
            .all(|d| !d.has_body)
    );
}

struct Failed;
impl MetadataProvider for Failed {
    fn metadata(&self, request: &MetadataRequest) -> Result<ProjectMetadata, MetadataError> {
        Err(MetadataError::new(
            "provider_unavailable",
            request.snapshot.clone(),
            "missing provider",
        ))
    }
}

#[test]
fn ifds_k017_provider_failure() {
    let mut request = request();
    let report = collect_project_metadata(&request, &Failed).unwrap();
    assert!(report.metadata.is_none());
    assert_eq!(
        report.diagnostics[0].code,
        DiagnosticCode::TypeResolutionUnavailable
    );
    request.files.remove("src/helper.ts");
    let missing = real(&request);
    assert_eq!(missing.calls[0].status, ResolutionStatus::MissingDependency);
    request.files.insert(
        "src/helper.ts".into(),
        MetadataFile {
            content_id: "body".into(),
            text: "export function helper(value: string) { return value; }".into(),
        },
    );
    request
        .options
        .excluded_paths
        .insert("src/helper.ts".into());
    let excluded = real(&request);
    assert_eq!(excluded.calls[0].status, ResolutionStatus::ExcludedBody);
}

#[test]
fn ifds_k017_safe_provider_request() {
    let mut request = request();
    request.files.insert(
        "package.json".into(),
        MetadataFile {
            content_id: "package".into(),
            text: r#"{"scripts":{"postinstall":"throw new Error('must not run')"}}"#.into(),
        },
    );
    request.files.get_mut("src/helper.ts").unwrap().text = "throw new Error('application must not execute'); export function helper(value: string) { return value; }".into();
    request.files.get_mut("tsconfig.json").unwrap().text = r##"{"compilerOptions":{"module":"esnext","moduleResolution":"bundler","paths":{"#/*":["./src/*"]},"plugins":[{"name":"nonexistent-plugin"}]},"include":["src"]}"##.into();
    let metadata = real(&request);
    assert_eq!(
        metadata
            .calls
            .iter()
            .find(|c| c.callee == "run")
            .unwrap()
            .status,
        ResolutionStatus::ResolvedBody
    );
    let mut traversal = request;
    traversal.options.supplied_files.insert(
        "../escape.json".into(),
        MetadataFile {
            content_id: "bad".into(),
            text: "{}".into(),
        },
    );
    assert_eq!(traversal.validate().unwrap_err().code, "invalid_request");
}

#[test]
fn ifds_k017_explicit_configs_and_no_typecheck_gate() {
    let mut request = request();
    request.files.get_mut("tsconfig.json").unwrap().text = r##"{"extends":"base-package/tsconfig.json","compilerOptions":{"module":"esnext","moduleResolution":"bundler","paths":{"#/*":["./src/*"]}},"include":["src"]}"##.into();
    let error = NodeMetadataProvider::default()
        .metadata(&request)
        .unwrap_err();
    assert_eq!(error.code, "missing_base_config");
    assert_eq!(error.requested_by.as_deref(), Some("tsconfig.json"));
    assert_eq!(
        error.specifier.as_deref(),
        Some("base-package/tsconfig.json")
    );
    request.options.supplied_files.insert(
        "node_modules/base-package/tsconfig.json".into(),
        MetadataFile {
            content_id: "provided-base".into(),
            text: "{\"compilerOptions\":{\"target\":\"es2022\"}}".into(),
        },
    );
    request.files.get_mut("src/main.ts").unwrap().text = "import { helper as run } from '#/barrel'; const count: number = 'wrong'; export function main(input: string) { return run(input); }".into();
    assert_eq!(
        real(&request).calls[0].status,
        ResolutionStatus::ResolvedBody
    );
}

#[test]
fn ifds_k017_unicode_and_nested_calls() {
    let mut request = request();
    request.files.get_mut("src/main.ts").unwrap().text = "// Unicode café 😀\r\nimport { helper as run } from '#/barrel';\r\nexport function main(input: string) { return run(run(input)); }".into();
    let metadata = real(&request);
    assert_eq!(metadata.calls.len(), 2);
    let text = &request.files["src/main.ts"].text;
    for call in &metadata.calls {
        assert!(
            text[call.span.byte_start as usize..call.span.byte_end as usize].starts_with("run(")
        );
        assert_eq!(call.span.start_line, 3);
    }
    let report = collect_project_metadata(&request, &NodeMetadataProvider::default()).unwrap();
    let nodes: BTreeSet<_> = report
        .procedure_calls
        .iter()
        .flat_map(|p| p.calls.iter().map(|c| &c.node))
        .collect();
    assert_eq!(nodes.len(), 2);
    request.files.get_mut("src/main.ts").unwrap().text = "import { helper as run } from '#/barrel';\r\nconst message = 'Unicode café 😀';\u{2028}\u{2029}\rexport function main(input: string) { return run(input); }".into();
    let metadata = real(&request);
    assert_eq!(metadata.calls.len(), 1);
    assert_eq!(metadata.calls[0].span.start_line, 2);
    assert_eq!(metadata.calls[0].span.end_line, 2);
    let text = &request.files["src/main.ts"].text;
    assert_eq!(
        &text[metadata.calls[0].span.byte_start as usize..metadata.calls[0].span.byte_end as usize],
        "run(input)"
    );
}

#[test]
fn ifds_k017_external_and_unknown_export() {
    let mut request = request();
    request.files.get_mut("src/main.ts").unwrap().text = "import { external } from 'some-package'; import { missing } from '#/helper'; export function main(input: string) { external(input); return missing(input); }".into();
    let metadata = real(&request);
    let external = metadata
        .calls
        .iter()
        .find(|c| c.callee == "external")
        .unwrap();
    assert_eq!(external.status, ResolutionStatus::ExternalPackage);
    assert_eq!(external.external_package.as_deref(), Some("some-package"));
    assert!(external.target.is_none());
    let missing = metadata
        .calls
        .iter()
        .find(|c| c.callee == "missing")
        .unwrap();
    assert_eq!(missing.status, ResolutionStatus::Unresolved);
    assert!(missing.target.is_none());
}

#[test]
fn ifds_k017_configs_discovery_and_missing() {
    let mut request = request();
    request.options.tsconfig = None;
    assert_eq!(real(&request).tsconfig, "tsconfig.json");
    request.files.insert(
        "tsconfig.web.json".into(),
        MetadataFile {
            content_id: "config".into(),
            text: "{\"extends\":\"./tsconfig.json\"}".into(),
        },
    );
    let error = NodeMetadataProvider::default()
        .metadata(&request)
        .unwrap_err();
    assert_eq!(error.code, "ambiguous_tsconfig");
    assert_eq!(&*error.candidates, &["tsconfig.json", "tsconfig.web.json"]);
    request.files.remove("tsconfig.web.json");
    request.files.remove("tsconfig.json");
    assert_eq!(
        NodeMetadataProvider::default()
            .metadata(&request)
            .unwrap_err()
            .code,
        "missing_tsconfig"
    );
    request.options.tsconfig = Some("tsconfig.json".into());
    request.options.supplied_files.insert("tsconfig.json".into(), MetadataFile { content_id: "explicit-config".into(), text: r##"{"compilerOptions":{"moduleResolution":"bundler","module":"esnext","paths":{"#/*":["./src/*"]}},"include":["src"]}"##.into() });
    assert_eq!(
        real(&request).calls[0].status,
        ResolutionStatus::ResolvedBody
    );
}

#[test]
fn ifds_k017_wildcard_namespace_and_overloads() {
    let mut request = request();
    request.files.get_mut("src/barrel.ts").unwrap().text = "export * from './helper';".into();
    request.files.get_mut("src/helper.ts").unwrap().text = "export function helper(value: string): string; export function helper(value: string) { return value; }".into();
    let metadata = real(&request);
    let call = &metadata.calls[0];
    assert_eq!(call.status, ResolutionStatus::ResolvedBody);
    assert!(target(&metadata, call).has_body);
    assert!(call.chain.iter().any(|step| step.kind == "reexport"));
    assert!(
        call.signature
            .as_ref()
            .unwrap()
            .declaration
            .as_ref()
            .unwrap()
            .byte_end
            < target(&metadata, call).span.byte_start
    );
    request.files.get_mut("src/main.ts").unwrap().text = "import * as utils from '#/barrel'; export function main(input: string) { return utils.helper(input); }".into();
    let namespace = real(&request);
    assert_eq!(namespace.calls[0].status, ResolutionStatus::ResolvedBody);
    assert_eq!(target(&namespace, &namespace.calls[0]).name, "helper");
    assert!(
        namespace.calls[0]
            .chain
            .iter()
            .any(|step| step.kind == "reexport")
    );
    request.files.get_mut("src/barrel.ts").unwrap().text = "export type * from './helper';".into();
    let type_namespace = real(&request);
    assert_eq!(type_namespace.calls[0].status, ResolutionStatus::Unresolved);
    assert!(type_namespace.calls[0].target.is_none());
    request.files.get_mut("src/main.ts").unwrap().text = "import { helper as run } from '#/barrel'; export function main(input: string) { return run(input); }".into();
    let type_named = real(&request);
    assert_eq!(
        type_named.calls[0].status,
        ResolutionStatus::UnsupportedDispatch
    );
    assert!(type_named.calls[0].target.is_none());
}

#[test]
fn ifds_k017_type_only_and_mutable_targets() {
    let mut request = request();
    request.files.get_mut("src/main.ts").unwrap().text = "import type { helper } from '#/helper'; export function main(input: string) { return helper(input); }".into();
    let type_only = real(&request);
    assert_eq!(
        type_only.calls[0].status,
        ResolutionStatus::UnsupportedDispatch
    );
    assert!(type_only.calls[0].target.is_none());
    request.files.get_mut("src/main.ts").unwrap().text = "import { helper } from '#/helper'; export function main(input: string) { return helper(input); }".into();
    request.files.get_mut("src/helper.ts").unwrap().text =
        "export function helper(value: string) { return value; } helper = (value) => 'changed';"
            .into();
    let changed = real(&request);
    assert_eq!(
        changed.calls[0].status,
        ResolutionStatus::UnsupportedDispatch
    );
    assert!(changed.calls[0].target.is_none());
}

#[test]
fn ifds_k017_program_and_function_alias_boundary() {
    let mut request = request();
    request.files.get_mut("src/main.ts").unwrap().text = "import { helper } from '#/helper'; const result = helper('de'); const alias = helper; alias('fr');".into();
    let report = collect_project_metadata(&request, &NodeMetadataProvider::default()).unwrap();
    let metadata = report.metadata.as_ref().unwrap();
    assert_eq!(
        metadata
            .calls
            .iter()
            .find(|c| c.callee == "helper")
            .unwrap()
            .status,
        ResolutionStatus::ResolvedBody
    );
    assert!(
        metadata
            .calls
            .iter()
            .find(|c| c.callee == "alias")
            .unwrap()
            .target
            .is_none()
    );
    assert_eq!(
        report
            .procedure_calls
            .iter()
            .filter(|p| p.path == "src/main.ts")
            .flat_map(|p| &p.calls)
            .count(),
        2
    );
    request.files.get_mut("src/main.ts").unwrap().text =
        "import { helper } from '#/helper'; class Holder { method() { return helper('fr'); } }"
            .into();
    let report = collect_project_metadata(&request, &NodeMetadataProvider::default()).unwrap();
    assert_eq!(report.metadata.as_ref().unwrap().calls.len(), 1);
    assert_eq!(report.unmapped_calls, vec![0]);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnresolvedCall
            && diagnostic
                .message
                .contains("no corresponding call operation")
    }));
}

/// Contract tests use independently authored fake responses; real compiler
/// resolution has its own integration gate.
struct RecordingProvider {
    requests: std::cell::RefCell<Vec<MetadataRequest>>,
}
impl MetadataProvider for RecordingProvider {
    fn metadata(&self, request: &MetadataRequest) -> Result<ProjectMetadata, MetadataError> {
        self.requests.borrow_mut().push(request.clone());
        let tsconfig = request.options.tsconfig.clone().unwrap();
        Ok(ProjectMetadata {
            protocol_version: METADATA_PROTOCOL_VERSION,
            compiler_version: METADATA_COMPILER_VERSION.into(),
            snapshot: request.snapshot.clone(),
            platform: request.options.platform.clone(),
            selected_paths: request.selected_paths.clone(),
            excluded_paths: request.options.excluded_paths.clone(),
            runtime_aliases: request.options.runtime_aliases.clone(),
            configuration_files: BTreeMap::from([(
                tsconfig.clone(),
                request.file(&tsconfig).unwrap().clone(),
            )]),
            tsconfig,
            compiler_options: serde_json::json!({}),
            declarations: Vec::new(),
            calls: Vec::new(),
            imports: Vec::new(),
            diagnostics: Vec::new(),
        })
    }
}

#[test]
fn ifds_k017_fake_provider_contract_and_failure_preserves_analysis() {
    use crate::ifds::*;
    let fixture = load_fixture(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ifds/fixtures/overwrite"),
    )
    .unwrap();
    let manifest = &fixture.manifest;
    let before = SnapshotHandle {
        repository_id: "fixture".into(),
        id: manifest.selector.snapshot.clone(),
    };
    let after = SnapshotHandle {
        repository_id: "fixture".into(),
        id: SnapshotId {
            side: SnapshotSide::After,
            revision: manifest.snapshots.after.revision.clone(),
            content_id: manifest.snapshots.after.content_id.clone(),
        },
    };
    let provider =
        InMemorySnapshotProvider::new([(&before, "before"), (&after, "after")].into_iter().map(
            |(handle, directory)| {
                let text = std::fs::read(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/ifds/fixtures/overwrite")
                        .join(directory)
                        .join("main.ts"),
                )
                .unwrap();
                InMemorySnapshot {
                    handle: handle.clone(),
                    files: BTreeMap::from([
                        ("main.ts".into(), (format!("{directory}-blob"), text)),
                        (
                            "tsconfig.json".into(),
                            ("config-blob".into(), b"{}".to_vec()),
                        ),
                    ]),
                }
            },
        ));
    let query = VariableFlowQuery {
        before: before.clone(),
        after: after.clone(),
        diff: RepositoryDiff {
            before_content_id: before.id.content_id.clone(),
            after_content_id: after.id.content_id.clone(),
            changes: BTreeSet::from([FileChange {
                kind: FileChangeKind::Modified,
                before_path: Some("main.ts".into()),
                after_path: Some("main.ts".into()),
                before_content_id: Some("before-blob".into()),
                after_content_id: Some("after-blob".into()),
            }]),
        },
        selected_binding: manifest.selector.clone(),
        counterpart: manifest.counterpart.clone(),
        entry: EntryPoint::ContainingFunction,
        capabilities: manifest.capabilities.clone(),
        summaries: manifest.model_versions.clone(),
        limits: manifest.limits.clone(),
    };
    let environment = AnalysisEnvironment {
        capabilities: query.capabilities.clone(),
        summaries: query.summaries.clone(),
    };
    let options = ProjectOptions {
        tsconfig: Some("tsconfig.json".into()),
        platform: "web".into(),
        ..ProjectOptions::default()
    };
    let pair = ProjectPairOptions {
        before: options.clone(),
        after: options,
    };
    let recorder = RecordingProvider {
        requests: Default::default(),
    };
    let report = analyze_variable_flow_with_metadata(
        query.clone(),
        &provider,
        (&environment, &environment),
        pair.clone(),
        &recorder,
    )
    .unwrap();
    compare_report(&fixture.expected, &report.analysis).unwrap();
    let requests = recorder.requests.borrow();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].snapshot, before.id);
    assert_eq!(requests[1].snapshot, after.id);
    assert_ne!(
        requests[0].files["main.ts"].text,
        requests[1].files["main.ts"].text
    );
    assert_eq!(
        requests[0].options.tsconfig.as_deref(),
        Some("tsconfig.json")
    );
    drop(requests);
    let failed = analyze_variable_flow_with_metadata(
        query.clone(),
        &provider,
        (&environment, &environment),
        pair.clone(),
        &Failed,
    )
    .unwrap();
    compare_report(&fixture.expected, &failed.analysis).unwrap();
    assert!(failed.before.metadata.is_none());
    assert_eq!(
        failed.before.diagnostics[0].code,
        DiagnosticCode::TypeResolutionUnavailable
    );
    let source = String::from_utf8(provider.read_file(&before, "main.ts").unwrap()).unwrap();
    let function = crate::ifds::adapters::typescript::index_functions("main.ts", &source)
        .unwrap()
        .remove(0);
    let function_query = FunctionResultQuery {
        before,
        after,
        diff: query.diff,
        selected_function: FunctionSelector {
            snapshot: query.selected_binding.snapshot,
            declaration: function.span,
            expected_name: function.name,
            expected_enclosing_symbol: function.owner,
        },
        counterpart: None,
        before_entry: FunctionEntry::default(),
        after_entry: FunctionEntry::default(),
        capabilities: query.capabilities,
        summaries: query.summaries,
        limits: query.limits,
    };
    let expected = analyze_function_result_reports(
        function_query.clone(),
        &provider,
        &environment,
        &environment,
    )
    .unwrap()
    .0;
    let with_metadata = analyze_function_result_with_metadata(
        function_query,
        &provider,
        (&environment, &environment),
        pair,
        &recorder,
    )
    .unwrap();
    assert_eq!(with_metadata.analysis, expected);
}

#[test]
fn ifds_k017_default_export_implementation_and_config_cycles() {
    let mut request = request();
    request.files.get_mut("src/main.ts").unwrap().text =
        "import run from '#/helper'; export function main(input: string) { return run(input); }"
            .into();
    request.files.get_mut("src/helper.ts").unwrap().text =
        "export default (value: string) => value;".into();
    let report = collect_project_metadata(&request, &NodeMetadataProvider::default()).unwrap();
    let metadata = report.metadata.as_ref().unwrap();
    let call = metadata.calls.iter().find(|c| c.callee == "run").unwrap();
    assert_eq!(call.status, ResolutionStatus::ResolvedBody);
    assert!(
        report
            .procedure_declarations
            .iter()
            .any(|p| Some(&p.declaration) == call.target.as_ref())
    );
    request.files.get_mut("tsconfig.json").unwrap().text =
        "{\"extends\":\"./tsconfig.json\"}".into();
    assert_eq!(
        NodeMetadataProvider::default()
            .metadata(&request)
            .unwrap_err()
            .code,
        "invalid_config"
    );
}

#[test]
fn ifds_k017_provider_timeout_and_missing_node() {
    let request = request();
    let unavailable = NodeMetadataProvider {
        node_executable: "/nonexistent/repo-memory-k017-node".into(),
        ..NodeMetadataProvider::default()
    };
    assert_eq!(
        unavailable.metadata(&request).unwrap_err().code,
        "provider_unavailable"
    );
    let timed_out = NodeMetadataProvider {
        timeout: std::time::Duration::ZERO,
        ..NodeMetadataProvider::default()
    };
    assert_eq!(
        timed_out.metadata(&request).unwrap_err().code,
        "provider_unavailable"
    );
}

#[test]
fn ifds_k017_project_reference_configs_are_explicit() {
    let mut request = request();
    request.files.get_mut("tsconfig.json").unwrap().text =
        "{\"references\":[{\"path\":\"./package\"}],\"files\":[]}".into();
    let error = NodeMetadataProvider::default()
        .metadata(&request)
        .unwrap_err();
    assert_eq!(error.code, "missing_tsconfig");
    assert_eq!(error.specifier.as_deref(), Some("./package"));
    request.files.insert(
        "package/tsconfig.json".into(),
        MetadataFile {
            content_id: "package-config".into(),
            text: "{\"compilerOptions\":{\"composite\":true},\"files\":[]}".into(),
        },
    );
    let metadata = real(&request);
    assert!(
        metadata
            .configuration_files
            .contains_key("package/tsconfig.json")
    );
}
