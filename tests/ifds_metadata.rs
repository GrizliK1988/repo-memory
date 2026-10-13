use repo_memory::ifds::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn bluesky_request() -> MetadataRequest {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ifds/metadata/bluesky-web");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let files = manifest["files"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(path, info)| {
            (
                path.clone(),
                MetadataFile {
                    content_id: info["git_blob"].as_str().unwrap().into(),
                    text: std::fs::read_to_string(root.join("snapshot").join(path)).unwrap(),
                },
            )
        })
        .collect();
    let base = "node_modules/@react-native/typescript-config/tsconfig.json";
    MetadataRequest {
        protocol_version: METADATA_PROTOCOL_VERSION,
        compiler_version: METADATA_COMPILER_VERSION.into(),
        snapshot: SnapshotId {
            side: SnapshotSide::After,
            revision: manifest["revision"].as_str().unwrap().into(),
            content_id: "pinned-bluesky-partial-slice".into(),
        },
        files,
        selected_paths: BTreeSet::from([
            "src/lib/api/feed/custom.ts".into(),
            "src/lib/strings/errors.ts".into(),
        ]),
        options: ProjectOptions {
            tsconfig: Some("tsconfig.check.web.json".into()),
            platform: "web".into(),
            supplied_files: BTreeMap::from([(
                base.into(),
                MetadataFile {
                    content_id: manifest["base_config"]["sha256"].as_str().unwrap().into(),
                    text: std::fs::read_to_string(root.join("supplied").join(base)).unwrap(),
                },
            )]),
            runtime_aliases: BTreeMap::from([
                ("#/*".into(), "src/*".into()),
                ("crypto".into(), "src/platform/crypto.ts".into()),
            ]),
            ..ProjectOptions::default()
        },
    }
}

#[test]
fn ifds_k017_bluesky_web_aliases_and_ir() {
    let request = bluesky_request();
    let report = collect_project_metadata(&request, &NodeMetadataProvider::default()).unwrap();
    let metadata = report.metadata.as_ref().unwrap();
    assert_eq!(
        metadata.compiler_options["moduleSuffixes"],
        serde_json::json!([".web", ""])
    );
    assert_eq!(metadata.configuration_files.len(), 4); // web, root, base, package.json
    let content_calls: Vec<_> = metadata
        .calls
        .iter()
        .filter(|c| c.callee == "getContentLanguages")
        .collect();
    assert_eq!(content_calls.len(), 2);
    for call in content_calls.into_iter().chain(
        metadata
            .calls
            .iter()
            .filter(|c| c.callee == "getAppLanguageAsContentLanguage"),
    ) {
        assert_eq!(call.status, ResolutionStatus::ResolvedBody);
        let declaration = metadata
            .declarations
            .iter()
            .find(|d| Some(&d.id) == call.target.as_ref())
            .unwrap();
        assert_eq!(declaration.span.path, "src/state/preferences/languages.tsx");
        assert_eq!(declaration.name, call.callee);
        assert!(
            call.chain
                .iter()
                .any(|step| step.specifier.as_deref() == Some("#/state/preferences/languages"))
        );
    }
    let xrpc_calls: Vec<_> = metadata
        .calls
        .iter()
        .enumerate()
        .filter(|(_, c)| c.callee == "isXrpcError" && c.span.path == "src/lib/strings/errors.ts")
        .collect();
    assert_eq!(xrpc_calls.len(), 2);
    let mut nodes = BTreeSet::new();
    for (index, call) in xrpc_calls {
        assert_eq!(call.status, ResolutionStatus::ResolvedBody);
        let binding = report
            .procedure_calls
            .iter()
            .filter(|p| p.path == "src/lib/strings/errors.ts")
            .flat_map(|p| &p.calls)
            .find(|b| b.call_index == index)
            .unwrap();
        nodes.insert(binding.node.clone());
        assert!(
            report
                .procedure_declarations
                .iter()
                .any(|p| Some(&p.declaration) == call.target.as_ref()
                    && p.path == "src/lib/xrpc-error.ts")
        );
    }
    assert_eq!(nodes.len(), 2);
    assert!(metadata.calls.iter().any(|c| c.callee == "getMain"
        && c.status == ResolutionStatus::ExternalPackage
        && c.external_package.as_deref() == Some("@atproto/lex")));
    assert!(metadata.imports.iter().any(|i| i.specifier == "#/lexicons" && i.status == ResolutionStatus::MissingDependency));
    assert!(!metadata.render_text(false).contains("  src/"));
    assert!(
        metadata
            .render_text(true)
            .contains("#/state/preferences/languages")
    );
}

#[test]
fn ifds_k017_cli_structured_config_error() {
    let mut request = bluesky_request();
    request.options.supplied_files.clear();
    let path =
        std::env::temp_dir().join(format!("repo-memory-k017-cli-{}.json", std::process::id()));
    std::fs::write(&path, serde_json::to_vec(&request).unwrap()).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_repo-memory"))
        .args(["metadata", "--request"])
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(!output.status.success());
    let error: MetadataError = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error.code, "missing_base_config");
    assert_eq!(
        error.specifier.as_deref(),
        Some("@react-native/typescript-config/tsconfig.json")
    );
    assert_eq!(*error.snapshot, request.snapshot);
}

#[test]
fn ifds_k017_cli_explicit_base_and_missing_node() {
    let mut request = bluesky_request();
    request.options.supplied_files.clear();
    let path = std::env::temp_dir().join(format!(
        "repo-memory-k017-cli-base-{}.json",
        std::process::id()
    ));
    std::fs::write(&path, serde_json::to_vec(&request).unwrap()).unwrap();
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ifds/metadata/bluesky-web/supplied/node_modules/@react-native/typescript-config/tsconfig.json");
    let mapping = format!(
        "node_modules/@react-native/typescript-config/tsconfig.json={}",
        base.display()
    );
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_repo-memory"))
        .args(["metadata", "--request"])
        .arg(&path)
        .args(["--config-file", &mapping])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: SnapshotMetadataReport = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report.metadata.is_some());
    let unavailable = std::process::Command::new(env!("CARGO_BIN_EXE_repo-memory"))
        .args(["metadata", "--request"])
        .arg(&path)
        .args(["--node", "/nonexistent/k017-node"])
        .output()
        .unwrap();
    assert!(!unavailable.status.success());
    let error: MetadataError = serde_json::from_slice(&unavailable.stderr).unwrap();
    assert_eq!(error.code, "provider_unavailable");
    std::fs::remove_file(path).unwrap();
}
