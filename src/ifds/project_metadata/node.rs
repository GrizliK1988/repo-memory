use super::*;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct NodeMetadataProvider {
    pub node_executable: PathBuf,
    pub bridge: PathBuf,
    pub timeout: Duration,
    pub max_bytes: usize,
}

impl Default for NodeMetadataProvider {
    fn default() -> Self {
        Self {
            node_executable: "node".into(),
            bridge: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tools/typescript-metadata/bridge.cjs"),
            timeout: Duration::from_secs(30),
            max_bytes: 128 * 1024 * 1024,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    metadata: Option<Box<ProjectMetadata>>,
    error: Option<MetadataError>,
}

impl MetadataProvider for NodeMetadataProvider {
    fn metadata(&self, request: &MetadataRequest) -> Result<ProjectMetadata, MetadataError> {
        request.validate()?;
        let failure =
            |message| MetadataError::new("provider_unavailable", request.snapshot.clone(), message);
        let bytes = serde_json::to_vec(request).map_err(|e| failure(e.to_string()))?;
        if bytes.len() > self.max_bytes {
            return Err(failure("metadata request exceeds byte budget".into()));
        }
        let mut child = Command::new(&self.node_executable)
            .arg("--max-old-space-size=512")
            .arg(&self.bridge)
            .env_remove("NODE_OPTIONS")
            .env_remove("NODE_PATH")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                failure(format!(
                    "Node.js and the pinned bridge must be provisioned before analysis: {e}"
                ))
            })?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let max = self.max_bytes;
        let writer = thread::spawn(move || stdin.write_all(&bytes));
        let output_reader = thread::spawn(move || {
            let mut output = Vec::new();
            stdout
                .take(max as u64 + 1)
                .read_to_end(&mut output)
                .map(|_| output)
        });
        let error_reader = thread::spawn(move || {
            let mut output = Vec::new();
            stderr.take(65537).read_to_end(&mut output).map(|_| output)
        });
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if start.elapsed() < self.timeout => {
                    thread::sleep(Duration::from_millis(5))
                }
                result => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(failure(match result {
                        Err(e) => e.to_string(),
                        _ => "metadata provider exceeded time budget".into(),
                    }));
                }
            }
        };
        let written = writer
            .join()
            .map_err(|_| failure("provider writer failed".into()))?;
        let output = output_reader
            .join()
            .map_err(|_| failure("provider reader failed".into()))?
            .map_err(|e| failure(e.to_string()))?;
        let stderr = error_reader
            .join()
            .map_err(|_| failure("provider stderr reader failed".into()))?
            .map_err(|e| failure(e.to_string()))?;
        let status = status?;
        if !status.success() {
            return Err(failure(format!(
                "metadata bridge failed: {}",
                String::from_utf8_lossy(&stderr)
            )));
        }
        written.map_err(|e| failure(e.to_string()))?;
        if output.len() > self.max_bytes {
            return Err(failure("metadata response exceeds byte budget".into()));
        }
        let response = serde_json::from_slice::<Response>(&output).map_err(|e| {
            MetadataError::new("invalid_response", request.snapshot.clone(), e.to_string())
        })?;
        match (response.metadata, response.error) {
            (Some(metadata), None) => {
                metadata.validate(request)?;
                Ok(*metadata)
            }
            (None, Some(error)) => {
                if *error.snapshot != request.snapshot {
                    return Err(MetadataError::new(
                        "invalid_response",
                        request.snapshot.clone(),
                        "provider error snapshot mismatch",
                    ));
                }
                Err(error)
            }
            _ => Err(MetadataError::new(
                "invalid_response",
                request.snapshot.clone(),
                "provider must return exactly one metadata result or error",
            )),
        }
    }
}
