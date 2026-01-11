//! Data loading utilities for files and URLs.

use crate::{
    error::{LoadError, Result},
    source::{DataSource, SourceMetadata},
};
use occlusion::{ActiveStore, Store};
use serde::Deserialize;
use std::{path::PathBuf, sync::LazyLock, time::Duration, time::Instant};
use tracing::info;
use uuid::Uuid;

/// Default HTTP timeout in seconds.
const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 30;

static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    let timeout_secs = std::env::var("OCCLUSION_HTTP_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_HTTP_TIMEOUT_SECS);

    reqwest::Client::builder()
        .user_agent(concat!("occlusion/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(timeout_secs))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build HTTP client")
});

#[derive(Debug, Deserialize)]
struct CsvRecord {
    uuid: String,
    visibility_level: u8,
}

/// Parse CSV and build store from bytes (blocking, CPU-intensive).
fn build_from_bytes(content: impl AsRef<[u8]>) -> Result<ActiveStore> {
    let start = Instant::now();

    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(content.as_ref());

    let entries: Vec<(Uuid, u8)> = csv_reader
        .deserialize()
        .enumerate()
        .map(|(line_num, result)| {
            let record: CsvRecord = result?;
            let uuid = record
                .uuid
                .parse::<Uuid>()
                .map_err(|e| LoadError::InvalidFormat(format!("Line {}: {}", line_num + 2, e)))?;
            Ok((uuid, record.visibility_level))
        })
        .collect::<Result<_>>()?;

    info!(
        entries = entries.len(),
        elapsed_ms = start.elapsed().as_millis() as u64,
        "CSV parsed"
    );

    let start = Instant::now();
    let store = occlusion::build_store(entries)?;
    info!(
        uuid_count = store.len(),
        elapsed_ms = start.elapsed().as_millis() as u64,
        "store built"
    );

    Ok(store)
}

/// Run blocking build on tokio's blocking threadpool.
async fn spawn_build(content: Vec<u8>) -> Result<ActiveStore> {
    tokio::task::spawn_blocking(move || build_from_bytes(content))
        .await
        .map_err(|e| LoadError::InvalidFormat(format!("Task join error: {}", e)))?
}

/// Load store from a DataSource, optionally checking if it changed.
///
/// - If `old_metadata` is `None`, always loads and returns `Some`.
/// - If `old_metadata` is `Some`, returns `None` if unchanged.
pub async fn load(
    source: &DataSource,
    old_metadata: Option<&SourceMetadata>,
) -> Result<Option<(ActiveStore, SourceMetadata)>> {
    match source {
        DataSource::File(path) => load_file(path.clone(), old_metadata).await,
        DataSource::Url(url) => load_url(url, old_metadata).await,
    }
}

/// Load store from a file, optionally checking mtime.
async fn load_file(
    path: PathBuf,
    old_metadata: Option<&SourceMetadata>,
) -> Result<Option<(ActiveStore, SourceMetadata)>> {
    let new_metadata = SourceMetadata::from_file(&path)?;

    if let Some(old) = old_metadata {
        if !old.has_changed(&new_metadata) {
            return Ok(None);
        }
    }

    let content = tokio::task::spawn_blocking(move || std::fs::read(path))
        .await
        .map_err(|e| LoadError::InvalidFormat(format!("Task join error: {}", e)))??;

    let store = spawn_build(content).await?;
    Ok(Some((store, new_metadata)))
}

/// Load store from a URL, optionally with conditional headers.
async fn load_url(
    url: &str,
    old_metadata: Option<&SourceMetadata>,
) -> Result<Option<(ActiveStore, SourceMetadata)>> {
    let mut request = HTTP_CLIENT.get(url);

    if let Some(meta) = old_metadata {
        if let Some(etag) = &meta.etag {
            request = request.header("If-None-Match", etag);
        }
        if let Some(last_modified) = &meta.last_modified {
            request = request.header("If-Modified-Since", last_modified);
        }
    }

    let start = Instant::now();
    let response = request.send().await?;

    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(None);
    }

    if !response.status().is_success() {
        return Err(LoadError::HttpError(format!(
            "HTTP request failed with status: {}",
            response.status()
        )));
    }

    let new_metadata = SourceMetadata {
        mtime: None,
        etag: response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        last_modified: response
            .headers()
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
    };

    let content = response.bytes().await?.to_vec();
    info!(
        elapsed_ms = start.elapsed().as_millis() as u64,
        "HTTP fetch completed"
    );

    let store = spawn_build(content).await?;
    Ok(Some((store, new_metadata)))
}
