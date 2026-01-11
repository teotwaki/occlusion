//! Data loading utilities for files and URLs.

use crate::error::{LoadError, Result};
use crate::source::{DataSource, SourceMetadata};
use occlusion::{ActiveStore, Store};
use serde::Deserialize;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::LazyLock;
use std::time::Instant;
use tracing::info;
use uuid::Uuid;

static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent(concat!("occlusion/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("Failed to build HTTP client")
});

#[derive(Debug, Deserialize)]
struct CsvRecord {
    uuid: String,
    visibility_level: u8,
}

/// Core CSV parsing from any reader.
fn load_entries_from_reader<R: Read>(reader: R) -> Result<Vec<(Uuid, u8)>> {
    let start = Instant::now();

    let mut csv_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(reader);

    let mut entries = Vec::new();

    for (line_num, result) in csv_reader.deserialize().enumerate() {
        let record: CsvRecord = result?;

        let uuid = record
            .uuid
            .parse::<Uuid>()
            .map_err(|e| LoadError::InvalidFormat(format!("Line {}: {}", line_num + 2, e)))?;

        entries.push((uuid, record.visibility_level));
    }

    info!(
        entries = entries.len(),
        elapsed_ms = start.elapsed().as_millis() as u64,
        "CSV parsed"
    );

    Ok(entries)
}

/// Load entries from a CSV file.
fn load_entries_from_file<P: AsRef<Path>>(path: P) -> Result<Vec<(Uuid, u8)>> {
    let file = std::fs::File::open(path.as_ref())?;
    load_entries_from_reader(file)
}

/// Load the store from a DataSource (file or URL) asynchronously.
pub async fn load_from_source(source: &DataSource) -> Result<(ActiveStore, SourceMetadata)> {
    match source {
        DataSource::File(path) => {
            let metadata = SourceMetadata::from_file(path)?;
            let entries = load_entries_from_file(path)?;

            let start = Instant::now();
            let store = occlusion::build_store(entries)?;
            info!(
                uuid_count = store.len(),
                elapsed_ms = start.elapsed().as_millis() as u64,
                "store built"
            );

            Ok((store, metadata))
        }
        DataSource::Url(url) => {
            load_from_url(url, None)
                .await?
                .ok_or_else(|| LoadError::InvalidFormat("Initial load returned no data".into()))
        }
    }
}

/// Conditionally reload from a DataSource if it has changed.
///
/// Returns `Ok(None)` if the source hasn't changed (304 Not Modified for URLs,
/// or same mtime for files). Returns `Ok(Some(...))` with new data if changed.
pub async fn reload_if_changed(
    source: &DataSource,
    old_metadata: &SourceMetadata,
) -> Result<Option<(ActiveStore, SourceMetadata)>> {
    match source {
        DataSource::File(path) => {
            let new_metadata = SourceMetadata::from_file(path)?;
            if !old_metadata.has_changed(&new_metadata) {
                return Ok(None);
            }
            let entries = load_entries_from_file(path)?;

            let start = Instant::now();
            let store = occlusion::build_store(entries)?;
            info!(
                uuid_count = store.len(),
                elapsed_ms = start.elapsed().as_millis() as u64,
                "store built"
            );

            Ok(Some((store, new_metadata)))
        }
        DataSource::Url(url) => load_from_url(url, Some(old_metadata)).await,
    }
}

/// Extract metadata from HTTP response headers.
fn extract_metadata_from_headers(headers: &reqwest::header::HeaderMap) -> SourceMetadata {
    SourceMetadata {
        mtime: None,
        etag: headers
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        last_modified: headers
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
    }
}

/// Load from a URL, optionally with conditional headers.
///
/// If `old_metadata` is provided, adds If-None-Match and If-Modified-Since headers.
/// Returns `Ok(None)` on 304 Not Modified, `Ok(Some(...))` on success.
async fn load_from_url(
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

    let fetch_start = Instant::now();
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

    let metadata = extract_metadata_from_headers(response.headers());
    let text = response.text().await?;
    info!(
        elapsed_ms = fetch_start.elapsed().as_millis() as u64,
        "HTTP fetch completed"
    );

    let entries = load_entries_from_reader(Cursor::new(text))?;

    let start = Instant::now();
    let store = occlusion::build_store(entries)?;
    info!(
        uuid_count = store.len(),
        elapsed_ms = start.elapsed().as_millis() as u64,
        "store built"
    );

    Ok(Some((store, metadata)))
}
