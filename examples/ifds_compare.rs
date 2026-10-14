//! Compare a selected binding or function in local TypeScript files or directories.

use repo_memory::ifds::adapters::typescript::{index_bindings, index_functions};
use repo_memory::ifds::{
    AnalysisEnvironment, AnalysisLimits, BindingSelector, CapabilitySet, EntryPoint, FileChange,
    FileChangeKind, FunctionEntry, FunctionPresentationOptions, FunctionResultQuery,
    FunctionSelector, InMemorySnapshot, InMemorySnapshotProvider, MetadataFile,
    NodeMetadataProvider, ProjectOptions, ProjectPairOptions, RepositoryDiff, SnapshotHandle,
    SnapshotId, SnapshotSide, VariableFlowQuery, analyze_function_result_reports,
    analyze_function_result_with_metadata, analyze_variable_flow_reports,
    compact_function_result_report,
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
    let usage = "usage: cargo run --example ifds_compare -- <before.ts> <after.ts> <binding> [--format text|compact-json|full-json] [--evidence-out <path>] [--verbose]\n   or: cargo run --example ifds_compare -- <before.ts> <after.ts> --function <name> [--after-function <name>] [--metadata] [--format text|compact-json|full-json] [--evidence-out <path>] [--verbose]\n   or: cargo run --example ifds_compare -- <before-dir> <after-dir> --function <name> --metadata [--entry-file main.ts] [--after-function <name>] [--format text|compact-json|full-json] [--evidence-out <path>] [--verbose]";
    if arguments.len() < 3 {
        return Err(usage.into());
    }
    let (before_path, after_path) = (&arguments[0], &arguments[1]);
    let binding_name = (!arguments[2].starts_with("--")).then_some(arguments[2].as_str());
    let mut function_name = None;
    let mut after_function_name = None;
    let mut format = "text";
    let mut evidence_out = None;
    let mut verbose = false;
    let mut metadata = false;
    let mut entry_file = None;
    let mut index = if binding_name.is_some() { 3 } else { 2 };
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--metadata" => {
                metadata = true;
                index += 1;
            }
            "--verbose" => {
                verbose = true;
                index += 1;
            }
            "--function" if index + 1 < arguments.len() => {
                function_name = Some(arguments[index + 1].as_str());
                index += 2;
            }
            "--after-function" if index + 1 < arguments.len() => {
                after_function_name = Some(arguments[index + 1].as_str());
                index += 2;
            }
            "--entry-file" if index + 1 < arguments.len() => {
                entry_file = Some(arguments[index + 1].as_str());
                index += 2;
            }
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
    if binding_name.is_some() == function_name.is_some()
        || (after_function_name.is_some() && function_name.is_none())
        || (metadata && function_name.is_none())
    {
        return Err(usage.into());
    }
    if verbose && format != "text" {
        return Err("--verbose requires --format text".into());
    }
    let directories = Path::new(before_path).is_dir() && Path::new(after_path).is_dir();
    if directories && (!metadata || function_name.is_none()) {
        return Err("directory inputs require --function and --metadata".into());
    }
    if !directories && entry_file.is_some() {
        return Err("--entry-file requires directory inputs".into());
    }
    let before_extension = Path::new(before_path)
        .extension()
        .and_then(|value| value.to_str());
    let after_extension = Path::new(after_path)
        .extension()
        .and_then(|value| value.to_str());
    if !directories
        && (before_extension != after_extension || !matches!(before_extension, Some("ts" | "tsx")))
    {
        return Err("both inputs must have the same .ts or .tsx extension".into());
    }
    let report_path = if directories {
        entry_file.unwrap_or("main.ts").to_owned()
    } else {
        format!("snippet.{}", before_extension.expect("validated extension"))
    };
    let before_files = load_files(Path::new(before_path), &report_path)?;
    let after_files = load_files(Path::new(after_path), &report_path)?;
    let before = &before_files
        .get(&report_path)
        .ok_or_else(|| format!("entry file {report_path:?} not found in {before_path}"))?
        .1;
    let after = &after_files
        .get(&report_path)
        .ok_or_else(|| format!("entry file {report_path:?} not found in {after_path}"))?
        .1;
    let before_text = std::str::from_utf8(before)?;
    let after_text = std::str::from_utf8(after)?;
    let before_hash = snapshot_digest(&before_files)?;
    let after_hash = snapshot_digest(&after_files)?;
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
    let paths: BTreeSet<_> = before_files.keys().chain(after_files.keys()).collect();
    let changes = paths
        .into_iter()
        .filter_map(|path| {
            let before = before_files.get(path);
            let after = after_files.get(path);
            if before == after {
                return None;
            }
            Some(FileChange {
                kind: match (before, after) {
                    (None, Some(_)) => FileChangeKind::Added,
                    (Some(_), None) => FileChangeKind::Deleted,
                    _ => FileChangeKind::Modified,
                },
                before_path: before.map(|_| path.clone()),
                after_path: after.map(|_| path.clone()),
                before_content_id: before.map(|(hash, _)| hash.clone()),
                after_content_id: after.map(|(hash, _)| hash.clone()),
            })
        })
        .collect();
    let provider = InMemorySnapshotProvider::new([
        InMemorySnapshot {
            handle: before_handle.clone(),
            files: before_files.clone(),
        },
        InMemorySnapshot {
            handle: after_handle.clone(),
            files: after_files.clone(),
        },
    ]);
    let capabilities = CapabilitySet {
        stage: if metadata { 4 } else { 3 },
        capabilities: BTreeSet::from([
            "straight_line".into(),
            "branches".into(),
            "short_circuit".into(),
            "loops".into(),
        ]),
        version: if metadata { "stage-4" } else { "stage-3" }.into(),
    };
    let environment = AnalysisEnvironment {
        capabilities: capabilities.clone(),
        summaries: BTreeSet::new(),
    };
    let diff = RepositoryDiff {
        before_content_id: before_hash,
        after_content_id: after_hash,
        changes,
    };
    let limits = AnalysisLimits {
        time_ms: 120_000,
        memory_bytes: 4_294_967_296,
        processed_path_edges: 1_000_000,
        witnesses_per_relation: 3,
        output_nodes: 100_000,
    };
    if let Some(name) = function_name {
        let selected = function_selector(&before_id, &report_path, before_text, name)?;
        let counterpart = after_function_name
            .map(|name| function_selector(&after_id, &report_path, after_text, name))
            .transpose()?;
        let query = FunctionResultQuery {
            before: before_handle,
            after: after_handle,
            diff,
            selected_function: selected,
            counterpart,
            before_entry: FunctionEntry::default(),
            after_entry: FunctionEntry::default(),
            capabilities,
            summaries: BTreeSet::new(),
            limits,
        };
        let (report, mut compact) = if metadata {
            let config = r#"{"compilerOptions":{"target":"es2022","module":"esnext","moduleResolution":"bundler"},"include":["**/*.ts","**/*.tsx"]}"#;
            let options = ProjectOptions {
                tsconfig: Some("tsconfig.json".into()),
                supplied_files: BTreeMap::from([(
                    "tsconfig.json".into(),
                    MetadataFile {
                        content_id: digest(config.as_bytes()),
                        text: config.into(),
                    },
                )]),
                platform: "web".into(),
                ..ProjectOptions::default()
            };
            let project = analyze_function_result_with_metadata(
                query,
                &provider,
                (&environment, &environment),
                ProjectPairOptions {
                    before: options.clone(),
                    after: options,
                },
                &NodeMetadataProvider::default(),
            )?;
            if project.before.metadata.is_none() || project.after.metadata.is_none() {
                return Err(format!(
                    "metadata unavailable: {}\n{}",
                    project.before.render_text(false),
                    project.after.render_text(false)
                )
                .into());
            }
            let compact = compact_function_result_report(
                &project.analysis,
                FunctionPresentationOptions::default(),
            );
            (project.analysis, compact)
        } else {
            analyze_function_result_reports(query, &provider, &environment, &environment)?
        };
        if format == "full-json" {
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        let path = evidence_out
            .cloned()
            .unwrap_or_else(|| compact.evidence_file.clone());
        compact.evidence_file = path.clone();
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
            println!(
                "{}",
                if verbose {
                    compact.render_verbose_text(&report)
                } else {
                    compact.render_text(&report)
                }
            );
        }
        return Ok(());
    }
    let binding_name = binding_name.expect("validated binding target");
    let before_index = index_bindings(before_id.clone(), &report_path, before_text)?;
    let after_index = index_bindings(after_id.clone(), &report_path, after_text)?;
    let selected_binding = selector(&before_index, binding_name)?;
    let counterpart = selector(&after_index, binding_name)?;
    let query = VariableFlowQuery {
        before: before_handle,
        after: after_handle,
        diff,
        selected_binding,
        counterpart: Some(counterpart),
        entry: EntryPoint::ContainingFunction,
        capabilities,
        summaries: BTreeSet::new(),
        limits,
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
    report.human_summary = compact.render_legacy_text();
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
        println!(
            "{}",
            if verbose {
                compact.render_verbose_text(&report)
            } else {
                compact.render_text(&report)
            }
        );
    }
    Ok(())
}

fn function_selector(
    snapshot: &SnapshotId,
    path: &str,
    source: &str,
    name: &str,
) -> Result<FunctionSelector, Box<dyn Error>> {
    let index = index_functions(path, source)?;
    let mut matches = index
        .iter()
        .filter(|function| function.name.as_deref() == Some(name));
    let Some(function) = matches.next() else {
        return Err(format!("function {name:?} was not found in {path}").into());
    };
    if matches.next().is_some() {
        return Err(format!(
            "multiple functions named {name:?} in {path}; use the exact source-span API"
        )
        .into());
    }
    Ok(FunctionSelector {
        snapshot: snapshot.clone(),
        declaration: function.span.clone(),
        expected_name: function.name.clone(),
        expected_enclosing_symbol: function.owner.clone(),
    })
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

type SnapshotFiles = BTreeMap<String, (String, Vec<u8>)>;

fn load_files(input: &Path, report_path: &str) -> Result<SnapshotFiles, Box<dyn Error>> {
    if !input.is_dir() {
        let bytes = std::fs::read(input)?;
        return Ok(BTreeMap::from([(
            report_path.into(),
            (digest(&bytes), bytes),
        )]));
    }
    let mut files = BTreeMap::new();
    collect_files(input, input, &mut files)?;
    Ok(files)
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut SnapshotFiles,
) -> Result<(), Box<dyn Error>> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if !matches!(entry.file_name().to_str(), Some("node_modules" | ".git")) {
                collect_files(root, &path, files)?;
            }
        } else if kind.is_file()
            && matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("ts" | "tsx")
            )
        {
            let bytes = std::fs::read(&path)?;
            let relative = path.strip_prefix(root)?.to_str().ok_or("non-UTF-8 path")?;
            files.insert(relative.replace('\\', "/"), (digest(&bytes), bytes));
        }
    }
    Ok(())
}

fn snapshot_digest(files: &SnapshotFiles) -> Result<String, Box<dyn Error>> {
    // Preserve the identity of existing single-file examples.
    if files.len() == 1 {
        return Ok(files.values().next().expect("one file").0.clone());
    }
    let identities: BTreeMap<_, _> = files.iter().map(|(path, (hash, _))| (path, hash)).collect();
    Ok(digest(&serde_json::to_vec(&identities)?))
}

fn digest(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("fnv1a64-{hash:016x}")
}
