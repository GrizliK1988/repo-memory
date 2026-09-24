//! Run the stage-1 variable-flow analyzer on two local TypeScript snippets.

use repo_memory::ifds::adapters::typescript::index_bindings;
use repo_memory::ifds::{
    AnalysisEnvironment, AnalysisLimits, BindingSelector, CapabilitySet, EntryPoint, FileChange,
    FileChangeKind, InMemorySnapshot, InMemorySnapshotProvider, RepositoryDiff, SnapshotHandle,
    SnapshotId, SnapshotSide, VariableFlowQuery, analyze_variable_flow_reports,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::path::Path;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let usage = "usage: cargo run --example ifds_compare -- <before.ts> <after.ts> <binding> [--format text|compact-json|full-json] [--evidence-out <path>]";
    if arguments.len() < 3 {
        return Err(usage.into());
    }
    let (before_path, after_path, binding_name) = (&arguments[0], &arguments[1], &arguments[2]);
    let mut format = "text";
    let mut evidence_out = None;
    let mut index = 3;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--format" if index + 1 < arguments.len() => {
                format = &arguments[index + 1];
                index += 2;
            }
            "--evidence-out" if index + 1 < arguments.len() => {
                evidence_out = Some(&arguments[index + 1]);
                index += 2;
            }
            _ => return Err(usage.into()),
        }
    }
    if !matches!(format, "text" | "compact-json" | "full-json") {
        return Err(usage.into());
    }
    let before = std::fs::read(before_path)?;
    let after = std::fs::read(after_path)?;
    let before_text = std::str::from_utf8(&before)?;
    let after_text = std::str::from_utf8(&after)?;
    let before_extension = Path::new(before_path)
        .extension()
        .and_then(|value| value.to_str());
    let after_extension = Path::new(after_path)
        .extension()
        .and_then(|value| value.to_str());
    if before_extension != after_extension || !matches!(before_extension, Some("ts" | "tsx")) {
        return Err("both inputs must have the same .ts or .tsx extension".into());
    }
    let report_path = format!("snippet.{}", before_extension.expect("validated extension"));
    let before_hash = digest(&before);
    let after_hash = digest(&after);
    let before_id = SnapshotId {
        side: SnapshotSide::Before,
        revision: format!("local-before-{before_hash}"),
        content_id: before_hash.clone(),
    };
    let after_id = SnapshotId {
        side: SnapshotSide::After,
        revision: format!("local-after-{after_hash}"),
        content_id: after_hash.clone(),
    };
    let before_handle = SnapshotHandle {
        id: before_id.clone(),
        repository_id: "local-example".into(),
    };
    let after_handle = SnapshotHandle {
        id: after_id.clone(),
        repository_id: "local-example".into(),
    };
    let before_index = index_bindings(before_id.clone(), &report_path, before_text)?;
    let after_index = index_bindings(after_id.clone(), &report_path, after_text)?;
    let selected_binding = selector(&before_index, binding_name)?;
    let counterpart = selector(&after_index, binding_name)?;
    let changes = if before == after {
        BTreeSet::new()
    } else {
        BTreeSet::from([FileChange {
            kind: FileChangeKind::Modified,
            before_path: Some(report_path.clone()),
            after_path: Some(report_path.clone()),
            before_content_id: Some(before_hash.clone()),
            after_content_id: Some(after_hash.clone()),
        }])
    };
    let provider = InMemorySnapshotProvider::new([
        InMemorySnapshot {
            handle: before_handle.clone(),
            files: BTreeMap::from([(report_path.clone(), (before_hash.clone(), before))]),
        },
        InMemorySnapshot {
            handle: after_handle.clone(),
            files: BTreeMap::from([(report_path, (after_hash.clone(), after))]),
        },
    ]);
    let capabilities = CapabilitySet {
        stage: 1,
        capabilities: BTreeSet::from(["straight_line".into()]),
        version: "stage-1".into(),
    };
    let environment = AnalysisEnvironment {
        capabilities: capabilities.clone(),
        summaries: BTreeSet::new(),
    };
    let query = VariableFlowQuery {
        before: before_handle,
        after: after_handle,
        diff: RepositoryDiff {
            before_content_id: before_hash,
            after_content_id: after_hash,
            changes,
        },
        selected_binding,
        counterpart: Some(counterpart),
        entry: EntryPoint::ContainingFunction,
        capabilities,
        summaries: BTreeSet::new(),
        limits: AnalysisLimits {
            time_ms: 120_000,
            memory_bytes: 4_294_967_296,
            processed_path_edges: 1_000_000,
            witnesses_per_relation: 3,
            output_nodes: 100_000,
        },
    };
    let (mut report, mut compact) =
        analyze_variable_flow_reports(query, &provider, &environment, &environment)?;
    if format == "full-json" {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    let path = evidence_out
        .cloned()
        .unwrap_or_else(|| compact.evidence_file.clone());
    compact.evidence_file = path.clone();
    report.human_summary = compact.render_text();
    let bytes = serde_json::to_vec_pretty(&report)?;
    if let Ok(previous) = std::fs::read(&path) {
        if previous != bytes {
            return Err(
                format!("evidence path {path:?} already contains a different report").into(),
            );
        }
    } else {
        std::fs::write(&path, bytes)?;
    }
    if format == "compact-json" {
        println!("{}", serde_json::to_string_pretty(&compact)?);
    } else {
        println!("{}", compact.render_text());
    }
    Ok(())
}

fn selector(
    index: &repo_memory::ifds::adapters::typescript::TypeScriptBindingIndex,
    name: &str,
) -> Result<BindingSelector, Box<dyn Error>> {
    let mut matches = index.bindings.iter().filter(|binding| binding.name == name);
    let Some(binding) = matches.next() else {
        return Err(format!("declaration {name:?} was not found in {}", index.path).into());
    };
    if matches.next().is_some() {
        return Err(format!(
            "multiple declarations named {name:?} in {}; choose an unambiguous snippet",
            index.path
        )
        .into());
    }
    Ok(BindingSelector {
        snapshot: index.snapshot.clone(),
        declaration: binding.declaration.clone(),
        expected_name: Some(name.into()),
        expected_enclosing_symbol: binding.enclosing_symbol.clone(),
    })
}

fn digest(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("fnv1a64-{hash:016x}")
}
