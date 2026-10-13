use repo_memory::ifds::{
    MetadataFile, MetadataRequest, NodeMetadataProvider, collect_project_metadata,
};
use std::path::PathBuf;

fn main() {
    match run() {
        Ok(output) => println!("{output}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<String, String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let usage = "usage: repo-memory metadata --request <snapshot-request.json> [--tsconfig <virtual-path>] [--config-file <virtual-path>=<local.json>] [--node <path>] [--bridge <path>] [--format json|text] [--verbose]";
    if args.first().is_none_or(|arg| arg != "metadata") {
        return Err(usage.into());
    }
    let mut request_path = None;
    let mut format = "json";
    let mut verbose = false;
    let mut tsconfig = None;
    let mut config_files = Vec::new();
    let mut provider = NodeMetadataProvider::default();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--request" if index + 1 < args.len() => {
                request_path = Some(PathBuf::from(&args[index + 1]));
                index += 2;
            }
            "--node" if index + 1 < args.len() => {
                provider.node_executable = PathBuf::from(&args[index + 1]);
                index += 2;
            }
            "--bridge" if index + 1 < args.len() => {
                provider.bridge = PathBuf::from(&args[index + 1]);
                index += 2;
            }
            "--tsconfig" if index + 1 < args.len() => {
                tsconfig = Some(args[index + 1].clone());
                index += 2;
            }
            "--config-file" if index + 1 < args.len() => {
                config_files.push(args[index + 1].clone());
                index += 2;
            }
            "--format" if index + 1 < args.len() => {
                format = &args[index + 1];
                index += 2;
            }
            "--verbose" => {
                verbose = true;
                index += 1;
            }
            _ => return Err(usage.into()),
        }
    }
    if !matches!(format, "json" | "text") || (verbose && format != "text") {
        return Err(usage.into());
    }
    let bytes = std::fs::read(request_path.ok_or(usage)?).map_err(|e| e.to_string())?;
    let mut request: MetadataRequest = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if let Some(tsconfig) = tsconfig {
        request.options.tsconfig = Some(tsconfig);
    }
    for mapping in config_files {
        let (virtual_path, local_path) = mapping.split_once('=').ok_or(usage)?;
        if !virtual_path.ends_with(".json") {
            return Err("--config-file accepts JSON configuration files".into());
        }
        let text = std::fs::read_to_string(local_path).map_err(|e| e.to_string())?;
        // Content is also preserved and checked byte-for-byte in the protocol.
        let digest = text.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
        let file = MetadataFile {
            content_id: format!("provided-fnv1a64:{digest:016x}"),
            text,
        };
        if request
            .options
            .supplied_files
            .insert(virtual_path.into(), file)
            .is_some()
        {
            return Err(format!("duplicate supplied config: {virtual_path}"));
        }
    }
    let report = collect_project_metadata(&request, &provider)
        .map_err(|e| serde_json::to_string(&e).unwrap_or_else(|_| e.to_string()))?;
    if report.metadata.is_none() {
        let error = repo_memory::ifds::MetadataError::new(
            "provider_unavailable",
            request.snapshot.clone(),
            report
                .diagnostics
                .first()
                .map(|d| d.message.clone())
                .unwrap_or_else(|| "metadata provider unavailable".into()),
        );
        return Err(serde_json::to_string(&error).map_err(|e| e.to_string())?);
    }
    if format == "json" {
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())
    } else {
        Ok(report.render_text(verbose))
    }
}
