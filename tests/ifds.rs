use repo_memory::ifds::{
    AnalysisEnvironment, EntryPoint, FileChange, FileChangeKind, InMemorySnapshot,
    InMemorySnapshotProvider, RepositoryDiff, SnapshotHandle, SnapshotId, SnapshotSide,
    VariableFlowQuery, analyze_variable_flow, compare_report, discover_fixtures, load_fixture,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[test]
fn hand_authored_ifds_fixtures_are_discoverable() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/ifds/fixtures");
    let fixtures = discover_fixtures(&root).expect("the K003 fixture suite must not be empty");
    let mut case_ids = fixtures
        .iter()
        .map(|path| {
            load_fixture(path)
                .expect("fixture must load and validate")
                .manifest
                .case_id
        })
        .collect::<Vec<_>>();
    case_ids.sort();
    assert_eq!(
        case_ids,
        ["copy_then_overwrite", "full_downstream_chain", "overwrite"]
    );
}

#[test]
fn promoted_stage_one_fixtures_match_public_api() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/ifds/fixtures");
    for path in discover_fixtures(&root).unwrap() {
        let fixture = load_fixture(&path).unwrap();
        if !fixture.manifest.promoted {
            continue;
        }
        let manifest = &fixture.manifest;
        let before_id = SnapshotId {
            side: SnapshotSide::Before,
            revision: manifest.snapshots.before.revision.clone(),
            content_id: manifest.snapshots.before.content_id.clone(),
        };
        let after_id = SnapshotId {
            side: SnapshotSide::After,
            revision: manifest.snapshots.after.revision.clone(),
            content_id: manifest.snapshots.after.content_id.clone(),
        };
        let before_handle = SnapshotHandle {
            id: before_id.clone(),
            repository_id: manifest.case_id.clone(),
        };
        let after_handle = SnapshotHandle {
            id: after_id.clone(),
            repository_id: manifest.case_id.clone(),
        };
        let before_bytes = std::fs::read(
            path.join(&manifest.snapshots.before.directory)
                .join("main.ts"),
        )
        .unwrap();
        let after_bytes = std::fs::read(
            path.join(&manifest.snapshots.after.directory)
                .join("main.ts"),
        )
        .unwrap();
        let provider = InMemorySnapshotProvider::new([
            InMemorySnapshot {
                handle: before_handle.clone(),
                files: BTreeMap::from([("main.ts".into(), ("before-blob".into(), before_bytes))]),
            },
            InMemorySnapshot {
                handle: after_handle.clone(),
                files: BTreeMap::from([("main.ts".into(), ("after-blob".into(), after_bytes))]),
            },
        ]);
        let query = VariableFlowQuery {
            before: before_handle,
            after: after_handle,
            diff: RepositoryDiff {
                before_content_id: before_id.content_id,
                after_content_id: after_id.content_id,
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
            capabilities: manifest.capabilities.clone(),
            summaries: manifest.model_versions.clone(),
        };
        let report = analyze_variable_flow(query, &provider, &environment, &environment)
            .unwrap_or_else(|error| panic!("{}: {error}", manifest.case_id));
        compare_report(&fixture.expected, &report)
            .unwrap_or_else(|error| panic!("{}: {error}", manifest.case_id));
    }
}
