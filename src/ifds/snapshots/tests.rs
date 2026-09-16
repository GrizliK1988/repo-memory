use super::*;
use crate::ifds::{AnalysisLimits, EntryPoint, RepositoryDiff, SourceSpan};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

fn set<T: Ord>(values: impl IntoIterator<Item = T>) -> BTreeSet<T> {
    values.into_iter().collect()
}

fn snapshot(side: SnapshotSide, revision: &str, content_id: &str) -> SnapshotHandle {
    SnapshotHandle {
        id: SnapshotId {
            side,
            revision: revision.into(),
            content_id: content_id.into(),
        },
        repository_id: "repo".into(),
    }
}

fn selector(handle: &SnapshotHandle, path: &str, start: u64, end: u64) -> BindingSelector {
    BindingSelector {
        snapshot: handle.id.clone(),
        declaration: SourceSpan {
            path: path.into(),
            byte_start: start,
            byte_end: end,
            start_line: 1,
            end_line: 1,
        },
        expected_name: Some("é".into()),
        expected_enclosing_symbol: Some("f".into()),
    }
}

fn environment(version: &str) -> AnalysisEnvironment {
    AnalysisEnvironment {
        capabilities: CapabilitySet {
            stage: 0,
            capabilities: set(["snapshots".into()]),
            version: version.into(),
        },
        summaries: set([ModelVersion {
            name: "standard".into(),
            version: version.into(),
            digest: format!("model-{version}"),
        }]),
    }
}

fn fixture() -> (
    VariableFlowQuery,
    InMemorySnapshotProvider,
    AnalysisEnvironment,
) {
    let before = snapshot(SnapshotSide::Before, "before-revision", "before-tree");
    let after = snapshot(SnapshotSide::After, "after-revision", "after-tree");
    let before_files = BTreeMap::from([
        (
            "src/name.ts".into(),
            ("old-name".into(), "let é = 1;\n".into()),
        ),
        (
            "old.ts".into(),
            ("rename-old".into(), b"export {};\n".to_vec()),
        ),
    ]);
    let after_files = BTreeMap::from([
        (
            "src/name.ts".into(),
            ("new-name".into(), "let é = 2;\n".into()),
        ),
        (
            "new.ts".into(),
            ("rename-old".into(), b"export {};\n".to_vec()),
        ),
    ]);
    let provider = InMemorySnapshotProvider::new([
        InMemorySnapshot {
            handle: before.clone(),
            files: before_files,
        },
        InMemorySnapshot {
            handle: after.clone(),
            files: after_files,
        },
    ]);
    let env = environment("1");
    let query = VariableFlowQuery {
        before: before.clone(),
        after: after.clone(),
        diff: RepositoryDiff {
            before_content_id: before.id.content_id.clone(),
            after_content_id: after.id.content_id.clone(),
            changes: set([
                FileChange {
                    kind: FileChangeKind::Modified,
                    before_path: Some("src/name.ts".into()),
                    after_path: Some("src/name.ts".into()),
                    before_content_id: Some("old-name".into()),
                    after_content_id: Some("new-name".into()),
                },
                FileChange {
                    kind: FileChangeKind::Renamed,
                    before_path: Some("old.ts".into()),
                    after_path: Some("new.ts".into()),
                    before_content_id: Some("rename-old".into()),
                    after_content_id: Some("rename-old".into()),
                },
            ]),
        },
        selected_binding: selector(&before, "src/name.ts", 4, 6),
        counterpart: None,
        entry: EntryPoint::ContainingFunction,
        capabilities: env.capabilities.clone(),
        summaries: env.summaries.clone(),
        limits: AnalysisLimits {
            time_ms: 1_000,
            memory_bytes: 1_000_000,
            processed_path_edges: 10_000,
            witnesses_per_relation: 2,
            output_nodes: 1_000,
        },
    };
    (query, provider, env)
}

#[test]
fn ifds_k002_matching_diff() {
    let (query, provider, env) = fixture();
    assert_eq!(validate_query(&query, &provider, &env, &env), Ok(()));

    let mut bad_content = query.clone();
    let change = bad_content
        .diff
        .changes
        .iter()
        .find(|change| change.kind == FileChangeKind::Modified)
        .unwrap()
        .clone();
    bad_content.diff.changes.remove(&change);
    let mut change = change;
    change.after_content_id = Some("wrong".into());
    bad_content.diff.changes.insert(change);
    assert!(matches!(
        validate_query(&bad_content, &provider, &env, &env),
        Err(InputError::InvalidDiff(_))
    ));

    let mut bad_rename = query.clone();
    let rename = bad_rename
        .diff
        .changes
        .iter()
        .find(|change| change.kind == FileChangeKind::Renamed)
        .unwrap()
        .clone();
    bad_rename.diff.changes.remove(&rename);
    let mut rename = rename;
    rename.after_path = Some("src/name.ts".into());
    rename.after_content_id = Some("new-name".into());
    bad_rename.diff.changes.insert(rename);
    assert!(matches!(
        validate_query(&bad_rename, &provider, &env, &env),
        Err(InputError::InvalidDiff(_))
    ));
}

#[test]
fn ifds_k002_one_binding_only() {
    let (mut query, provider, env) = fixture();
    assert_eq!(validate_query(&query, &provider, &env, &env), Ok(()));

    query.counterpart = Some(query.selected_binding.clone());
    assert!(matches!(
        validate_query(&query, &provider, &env, &env),
        Err(InputError::InvalidSelector(_))
    ));

    let encoded = serde_json::to_value(&query).unwrap();
    let mut missing = encoded.as_object().unwrap().clone();
    missing.remove("selected_binding");
    assert!(serde_json::from_value::<VariableFlowQuery>(missing.into()).is_err());
}

#[test]
fn ifds_k002_stale_revision() {
    let (mut query, provider, env) = fixture();
    query.selected_binding.snapshot.revision = "some-other-revision".into();
    assert!(matches!(
        validate_query(&query, &provider, &env, &env),
        Err(InputError::SnapshotMismatch(_))
    ));
}

#[test]
fn ifds_k002_selector_assertion_contract() {
    let (query, provider, env) = fixture();
    validate_query(&query, &provider, &env, &env).unwrap();
    let round_trip: VariableFlowQuery =
        serde_json::from_slice(&serde_json::to_vec(&query).unwrap()).unwrap();
    assert_eq!(
        round_trip.selected_binding.expected_name.as_deref(),
        Some("é")
    );
    assert_eq!(
        round_trip
            .selected_binding
            .expected_enclosing_symbol
            .as_deref(),
        Some("f")
    );

    let mut omitted = query;
    omitted.selected_binding.expected_name = None;
    omitted.selected_binding.expected_enclosing_symbol = None;
    validate_query(&omitted, &provider, &env, &env).unwrap();
    let round_trip: VariableFlowQuery =
        serde_json::from_slice(&serde_json::to_vec(&omitted).unwrap()).unwrap();
    assert_eq!(round_trip.selected_binding.expected_name, None);
    assert_eq!(round_trip.selected_binding.expected_enclosing_symbol, None);
}

#[test]
fn ifds_k002_utf8_coordinates() {
    let (mut query, provider, env) = fixture();
    validate_query(&query, &provider, &env, &env).unwrap();
    assert_eq!(query.selected_binding.declaration.byte_start, 4);
    assert_eq!(query.selected_binding.declaration.byte_end, 6);

    query.selected_binding.declaration.byte_start = 5;
    assert!(matches!(
        validate_query(&query, &provider, &env, &env),
        Err(InputError::InvalidSelector(_))
    ));
    query.selected_binding.declaration.byte_start = 4;
    query.selected_binding.declaration.byte_end = 100;
    assert!(matches!(
        validate_query(&query, &provider, &env, &env),
        Err(InputError::InvalidSelector(_))
    ));
}

#[test]
fn ifds_k002_versions_match() {
    let (query, provider, env) = fixture();
    assert!(matches!(
        validate_query(&query, &provider, &env, &environment("2")),
        Err(InputError::CapabilityMismatch(_))
    ));

    let mut model_mismatch = env.clone();
    model_mismatch.summaries.clear();
    assert!(matches!(
        validate_query(&query, &provider, &env, &model_mismatch),
        Err(InputError::CapabilityMismatch(_))
    ));
}

#[test]
fn ifds_k002_no_workspace_mutation() {
    let (query, provider, env) = fixture();
    #[derive(Clone)]
    struct RecordingProvider {
        inner: InMemorySnapshotProvider,
        reads: Arc<Mutex<Vec<&'static str>>>,
    }
    impl SnapshotProvider for RecordingProvider {
        fn resolve(&self, handle: &SnapshotHandle) -> Result<SnapshotId, InputError> {
            self.reads.lock().unwrap().push("resolve");
            self.inner.resolve(handle)
        }

        fn files(&self, handle: &SnapshotHandle) -> Result<BTreeMap<String, String>, InputError> {
            self.reads.lock().unwrap().push("files");
            self.inner.files(handle)
        }

        fn read_file(&self, handle: &SnapshotHandle, path: &str) -> Result<Vec<u8>, InputError> {
            self.reads.lock().unwrap().push("read_file");
            self.inner.read_file(handle, path)
        }
    }
    let reads = Arc::new(Mutex::new(Vec::new()));
    let recording = RecordingProvider {
        inner: provider.clone(),
        reads: reads.clone(),
    };
    validate_query(&query, &recording, &env, &env).unwrap();
    assert_eq!(recording.inner.snapshots, provider.snapshots);
    assert_eq!(
        reads.lock().unwrap().as_slice(),
        ["resolve", "resolve", "files", "files", "read_file"]
    );
}

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn ifds_k002_git_provider_preserves_dirty_worktree() {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "repo-memory-k002-{}-{sequence}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    git(&directory, &["init", "-q"]);
    git(&directory, &["config", "user.name", "K002 Test"]);
    git(
        &directory,
        &["config", "user.email", "k002@example.invalid"],
    );
    fs::write(directory.join("source.ts"), "let value = 1;\n").unwrap();
    git(&directory, &["add", "source.ts"]);
    git(&directory, &["commit", "-q", "-m", "before"]);
    let before_revision = git(&directory, &["rev-parse", "HEAD"]);
    let before_tree = git(&directory, &["rev-parse", "HEAD^{tree}"]);
    let before_blob = git(&directory, &["rev-parse", "HEAD:source.ts"]);

    fs::write(directory.join("source.ts"), "let value = 2;\n").unwrap();
    git(&directory, &["add", "source.ts"]);
    git(&directory, &["commit", "-q", "-m", "after"]);
    let after_revision = git(&directory, &["rev-parse", "HEAD"]);
    let after_tree = git(&directory, &["rev-parse", "HEAD^{tree}"]);
    let after_blob = git(&directory, &["rev-parse", "HEAD:source.ts"]);
    fs::write(directory.join("unrelated.txt"), b"dirty\n").unwrap();
    let dirty_before = fs::read(directory.join("unrelated.txt")).unwrap();
    let status_before = git(&directory, &["status", "--porcelain=v1"]);

    let before = snapshot(SnapshotSide::Before, &before_revision, &before_tree);
    let after = snapshot(SnapshotSide::After, &after_revision, &after_tree);
    let provider = GitSnapshotProvider::new(BTreeMap::from([("repo".into(), directory.clone())]));
    assert_eq!(
        provider.read_file(&before, "source.ts").unwrap(),
        b"let value = 1;\n"
    );
    assert_eq!(
        provider.read_file(&after, "source.ts").unwrap(),
        b"let value = 2;\n"
    );
    assert_eq!(provider.files(&before).unwrap()["source.ts"], before_blob);
    assert_eq!(provider.files(&after).unwrap()["source.ts"], after_blob);
    assert_eq!(
        fs::read(directory.join("unrelated.txt")).unwrap(),
        dirty_before
    );
    assert_eq!(
        git(&directory, &["status", "--porcelain=v1"]),
        status_before
    );

    fs::remove_dir_all(directory).unwrap();
}
