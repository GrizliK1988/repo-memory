//! Immutable repository snapshots and validation performed before analysis.

use super::{
    BindingSelector, CapabilitySet, FileChange, FileChangeKind, InputError, ModelVersion,
    SnapshotHandle, SnapshotId, SnapshotSide, VariableFlowQuery,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// The effective analysis configuration used for one side of a comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisEnvironment {
    pub capabilities: CapabilitySet,
    pub summaries: BTreeSet<ModelVersion>,
}

/// A provider can only observe immutable repository objects.
pub trait SnapshotProvider {
    fn resolve(&self, handle: &SnapshotHandle) -> Result<SnapshotId, InputError>;
    fn files(&self, handle: &SnapshotHandle) -> Result<BTreeMap<String, String>, InputError>;
    fn read_file(&self, handle: &SnapshotHandle, path: &str) -> Result<Vec<u8>, InputError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InMemorySnapshot {
    pub handle: SnapshotHandle,
    /// Repository-relative path to `(content identity, bytes)`.
    pub files: BTreeMap<String, (String, Vec<u8>)>,
}

#[derive(Debug, Clone, Default)]
pub struct InMemorySnapshotProvider {
    snapshots: BTreeMap<(String, String), InMemorySnapshot>,
}

impl InMemorySnapshotProvider {
    pub fn new(snapshots: impl IntoIterator<Item = InMemorySnapshot>) -> Self {
        Self {
            snapshots: snapshots
                .into_iter()
                .map(|snapshot| {
                    (
                        (
                            snapshot.handle.repository_id.clone(),
                            snapshot.handle.id.revision.clone(),
                        ),
                        snapshot,
                    )
                })
                .collect(),
        }
    }

    fn get(&self, handle: &SnapshotHandle) -> Result<&InMemorySnapshot, InputError> {
        self.snapshots
            .get(&(handle.repository_id.clone(), handle.id.revision.clone()))
            .ok_or_else(|| {
                InputError::SnapshotMismatch(format!(
                    "revision {:?} is unavailable in repository {:?}",
                    handle.id.revision, handle.repository_id
                ))
            })
    }
}

impl SnapshotProvider for InMemorySnapshotProvider {
    fn resolve(&self, handle: &SnapshotHandle) -> Result<SnapshotId, InputError> {
        Ok(self.get(handle)?.handle.id.clone())
    }

    fn files(&self, handle: &SnapshotHandle) -> Result<BTreeMap<String, String>, InputError> {
        Ok(self
            .get(handle)?
            .files
            .iter()
            .map(|(path, (content_id, _))| (path.clone(), content_id.clone()))
            .collect())
    }

    fn read_file(&self, handle: &SnapshotHandle, path: &str) -> Result<Vec<u8>, InputError> {
        validate_path(path)?;
        self.get(handle)?
            .files
            .get(path)
            .map(|(_, bytes)| bytes.clone())
            .ok_or_else(|| InputError::InvalidSelector(format!("file {path:?} does not exist")))
    }
}

/// Reads commits, trees, and blobs without touching the index or worktree.
#[derive(Debug, Clone)]
pub struct GitSnapshotProvider {
    repositories: BTreeMap<String, PathBuf>,
}

impl GitSnapshotProvider {
    pub fn new(repositories: BTreeMap<String, PathBuf>) -> Self {
        Self { repositories }
    }

    fn repository(&self, handle: &SnapshotHandle) -> Result<&Path, InputError> {
        self.repositories
            .get(&handle.repository_id)
            .map(PathBuf::as_path)
            .ok_or_else(|| {
                InputError::SnapshotMismatch(format!(
                    "unknown repository {:?}",
                    handle.repository_id
                ))
            })
    }

    fn git(&self, handle: &SnapshotHandle, args: &[&str]) -> Result<Vec<u8>, InputError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(self.repository(handle)?)
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .map_err(|error| InputError::SnapshotMismatch(format!("cannot run git: {error}")))?;
        if !output.status.success() {
            return Err(InputError::SnapshotMismatch(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        Ok(output.stdout)
    }

    fn verify_revision(revision: &str) -> Result<(), InputError> {
        if !matches!(revision.len(), 40 | 64)
            || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(InputError::SnapshotMismatch(
                "revision must be a full Git object ID".into(),
            ));
        }
        Ok(())
    }
}

impl SnapshotProvider for GitSnapshotProvider {
    fn resolve(&self, handle: &SnapshotHandle) -> Result<SnapshotId, InputError> {
        Self::verify_revision(&handle.id.revision)?;
        let commit = String::from_utf8(self.git(
            handle,
            &[
                "rev-parse",
                "--verify",
                &format!("{}^{{commit}}", handle.id.revision),
            ],
        )?)
        .map_err(|_| InputError::SnapshotMismatch("Git returned a non-UTF-8 object ID".into()))?;
        let commit = commit.trim();
        if commit != handle.id.revision {
            return Err(InputError::SnapshotMismatch(format!(
                "revision resolved to {commit}, not {}",
                handle.id.revision
            )));
        }
        let tree = String::from_utf8(self.git(
            handle,
            &[
                "rev-parse",
                "--verify",
                &format!("{}^{{tree}}", handle.id.revision),
            ],
        )?)
        .map_err(|_| InputError::SnapshotMismatch("Git returned a non-UTF-8 tree ID".into()))?;
        let tree = tree.trim();
        if tree != handle.id.content_id {
            return Err(InputError::SnapshotMismatch(format!(
                "revision {} has tree {tree}, not {}",
                handle.id.revision, handle.id.content_id
            )));
        }
        Ok(handle.id.clone())
    }

    fn files(&self, handle: &SnapshotHandle) -> Result<BTreeMap<String, String>, InputError> {
        self.resolve(handle)?;
        let output = self.git(handle, &["ls-tree", "-r", "-z", &handle.id.revision])?;
        let mut files = BTreeMap::new();
        for record in output
            .split(|byte| *byte == 0)
            .filter(|record| !record.is_empty())
        {
            let tab = record
                .iter()
                .position(|byte| *byte == b'\t')
                .ok_or_else(|| InputError::SnapshotMismatch("invalid git ls-tree record".into()))?;
            let metadata = std::str::from_utf8(&record[..tab])
                .map_err(|_| InputError::SnapshotMismatch("non-UTF-8 tree metadata".into()))?;
            let path = std::str::from_utf8(&record[tab + 1..]).map_err(|_| {
                InputError::SnapshotMismatch("non-UTF-8 paths are unsupported".into())
            })?;
            let mut fields = metadata.split_ascii_whitespace();
            let _mode = fields.next();
            let kind = fields.next();
            let oid = fields.next();
            if kind == Some("blob") {
                files.insert(
                    path.to_owned(),
                    oid.ok_or_else(|| InputError::SnapshotMismatch("missing blob ID".into()))?
                        .to_owned(),
                );
            }
        }
        Ok(files)
    }

    fn read_file(&self, handle: &SnapshotHandle, path: &str) -> Result<Vec<u8>, InputError> {
        self.resolve(handle)?;
        validate_path(path)?;
        self.git(
            handle,
            &[
                "cat-file",
                "blob",
                &format!("{}:{path}", handle.id.revision),
            ],
        )
    }
}

pub fn validate_query<P: SnapshotProvider>(
    query: &VariableFlowQuery,
    provider: &P,
    before_environment: &AnalysisEnvironment,
    after_environment: &AnalysisEnvironment,
) -> Result<(), InputError> {
    validate_snapshot_pair(query, provider)?;
    validate_environments(query, before_environment, after_environment)?;
    validate_diff(query, provider)?;
    validate_selectors(query, provider)?;
    Ok(())
}

fn validate_snapshot_pair<P: SnapshotProvider>(
    query: &VariableFlowQuery,
    provider: &P,
) -> Result<(), InputError> {
    if query.before.repository_id != query.after.repository_id {
        return Err(InputError::SnapshotMismatch(
            "before and after snapshots belong to different repositories".into(),
        ));
    }
    if query.before.id.side != SnapshotSide::Before || query.after.id.side != SnapshotSide::After {
        return Err(InputError::SnapshotMismatch(
            "snapshot handles are assigned to the wrong sides".into(),
        ));
    }
    for handle in [&query.before, &query.after] {
        let resolved = provider.resolve(handle)?;
        if resolved != handle.id {
            return Err(InputError::SnapshotMismatch(format!(
                "provider resolved {:?}, expected {:?}",
                resolved, handle.id
            )));
        }
    }
    Ok(())
}

fn validate_environments(
    query: &VariableFlowQuery,
    before: &AnalysisEnvironment,
    after: &AnalysisEnvironment,
) -> Result<(), InputError> {
    if before != after {
        return Err(InputError::CapabilityMismatch(
            "before and after effective capabilities or model versions differ".into(),
        ));
    }
    if before.capabilities != query.capabilities || before.summaries != query.summaries {
        return Err(InputError::CapabilityMismatch(
            "effective analysis environment does not match the query".into(),
        ));
    }
    Ok(())
}

fn validate_diff<P: SnapshotProvider>(
    query: &VariableFlowQuery,
    provider: &P,
) -> Result<(), InputError> {
    if query.diff.before_content_id != query.before.id.content_id
        || query.diff.after_content_id != query.after.id.content_id
    {
        return Err(InputError::InvalidDiff(
            "diff is bound to different snapshot contents".into(),
        ));
    }
    let before_files = provider.files(&query.before)?;
    let after_files = provider.files(&query.after)?;
    let mut covered_before = BTreeSet::new();
    let mut covered_after = BTreeSet::new();
    for change in &query.diff.changes {
        validate_change(
            change,
            &before_files,
            &after_files,
            &mut covered_before,
            &mut covered_after,
        )?;
    }
    for path in before_files.keys().chain(after_files.keys()) {
        let before_id = before_files.get(path);
        let after_id = after_files.get(path);
        if before_id != after_id && !covered_before.contains(path) && !covered_after.contains(path)
        {
            return Err(InputError::InvalidDiff(format!(
                "changed path {path:?} is missing from the diff"
            )));
        }
    }
    Ok(())
}

fn validate_change(
    change: &FileChange,
    before_files: &BTreeMap<String, String>,
    after_files: &BTreeMap<String, String>,
    covered_before: &mut BTreeSet<String>,
    covered_after: &mut BTreeSet<String>,
) -> Result<(), InputError> {
    let expected_shape = match change.kind {
        FileChangeKind::Added => (false, true),
        FileChangeKind::Deleted => (true, false),
        FileChangeKind::Modified | FileChangeKind::Renamed => (true, true),
    };
    let actual_shape = (change.before_path.is_some(), change.after_path.is_some());
    if actual_shape != expected_shape
        || change.before_content_id.is_some() != expected_shape.0
        || change.after_content_id.is_some() != expected_shape.1
    {
        return Err(InputError::InvalidDiff(format!(
            "invalid {:?} record shape",
            change.kind
        )));
    }
    if change.kind == FileChangeKind::Modified && change.before_path != change.after_path {
        return Err(InputError::InvalidDiff(
            "modified records must keep the same path".into(),
        ));
    }
    if change.kind == FileChangeKind::Modified
        && change.before_content_id == change.after_content_id
    {
        return Err(InputError::InvalidDiff(
            "modified records must change file content".into(),
        ));
    }
    if change.kind == FileChangeKind::Renamed && change.before_path == change.after_path {
        return Err(InputError::InvalidDiff(
            "renamed records must change path".into(),
        ));
    }
    check_change_side(
        "before",
        change.before_path.as_deref(),
        change.before_content_id.as_deref(),
        before_files,
        covered_before,
    )?;
    check_change_side(
        "after",
        change.after_path.as_deref(),
        change.after_content_id.as_deref(),
        after_files,
        covered_after,
    )?;
    if let Some(path) = &change.before_path
        && matches!(
            change.kind,
            FileChangeKind::Deleted | FileChangeKind::Renamed
        )
        && after_files.contains_key(path)
    {
        return Err(InputError::InvalidDiff(format!(
            "old path {path:?} still exists after"
        )));
    }
    if let Some(path) = &change.after_path
        && matches!(change.kind, FileChangeKind::Added | FileChangeKind::Renamed)
        && before_files.contains_key(path)
    {
        return Err(InputError::InvalidDiff(format!(
            "new path {path:?} already exists before"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;

fn check_change_side(
    side: &str,
    path: Option<&str>,
    content_id: Option<&str>,
    files: &BTreeMap<String, String>,
    covered: &mut BTreeSet<String>,
) -> Result<(), InputError> {
    if let (Some(path), Some(content_id)) = (path, content_id) {
        validate_path(path)?;
        if !covered.insert(path.to_owned()) {
            return Err(InputError::InvalidDiff(format!(
                "{side} path {path:?} occurs more than once"
            )));
        }
        match files.get(path) {
            Some(actual) if actual == content_id => {}
            Some(actual) => {
                return Err(InputError::InvalidDiff(format!(
                    "{side} path {path:?} has content {actual}, not {content_id}"
                )));
            }
            None => {
                return Err(InputError::InvalidDiff(format!(
                    "{side} path {path:?} does not exist"
                )));
            }
        }
    }
    Ok(())
}

fn validate_selectors<P: SnapshotProvider>(
    query: &VariableFlowQuery,
    provider: &P,
) -> Result<(), InputError> {
    validate_selector(&query.selected_binding, query, provider)?;
    if let Some(counterpart) = &query.counterpart {
        if counterpart.snapshot.side == query.selected_binding.snapshot.side {
            return Err(InputError::InvalidSelector(
                "selected binding and counterpart must be on opposite sides".into(),
            ));
        }
        validate_selector(counterpart, query, provider)?;
    }
    Ok(())
}

fn validate_selector<P: SnapshotProvider>(
    selector: &BindingSelector,
    query: &VariableFlowQuery,
    provider: &P,
) -> Result<(), InputError> {
    let handle = match selector.snapshot.side {
        SnapshotSide::Before => &query.before,
        SnapshotSide::After => &query.after,
    };
    if selector.snapshot != handle.id {
        return Err(InputError::SnapshotMismatch(
            "selector snapshot does not match the query snapshot".into(),
        ));
    }
    selector
        .declaration
        .validate()
        .map_err(|error| InputError::InvalidSelector(error.to_string()))?;
    validate_path(&selector.declaration.path)?;
    if selector.declaration.byte_start == selector.declaration.byte_end {
        return Err(InputError::InvalidSelector(
            "declaration span must not be empty".into(),
        ));
    }
    let bytes = provider.read_file(handle, &selector.declaration.path)?;
    let start = usize::try_from(selector.declaration.byte_start)
        .map_err(|_| InputError::InvalidSelector("byte_start is too large".into()))?;
    let end = usize::try_from(selector.declaration.byte_end)
        .map_err(|_| InputError::InvalidSelector("byte_end is too large".into()))?;
    if end > bytes.len() {
        return Err(InputError::InvalidSelector(format!(
            "declaration span ends at {end}, beyond {} bytes",
            bytes.len()
        )));
    }
    let source = std::str::from_utf8(&bytes)
        .map_err(|_| InputError::InvalidSelector("selected source is not UTF-8".into()))?;
    if !source.is_char_boundary(start) || !source.is_char_boundary(end) {
        return Err(InputError::InvalidSelector(
            "declaration span splits a UTF-8 code point".into(),
        ));
    }
    Ok(())
}

fn validate_path(path: &str) -> Result<(), InputError> {
    let value = Path::new(path);
    if path.is_empty()
        || value.is_absolute()
        || value
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(InputError::InvalidSelector(format!(
            "path {path:?} must be a normalized repository-relative path"
        )));
    }
    Ok(())
}
