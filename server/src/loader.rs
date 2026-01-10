//! Data loading utilities for files and URLs.

use crate::error::{LoadError, Result};
use crate::source::{DataSource, SourceMetadata};
use occlusion::ActiveStore;
use serde::Deserialize;
use std::io::{Cursor, Read};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct CsvRecord {
    uuid: String,
    visibility_level: u8,
}

/// Core CSV parsing from any reader.
fn load_entries_from_reader<R: Read>(reader: R) -> Result<Vec<(Uuid, u8)>> {
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

    Ok(entries)
}

/// Load entries from a CSV file.
fn load_entries_from_file<P: AsRef<Path>>(path: P) -> Result<Vec<(Uuid, u8)>> {
    let file = std::fs::File::open(path.as_ref())?;
    load_entries_from_reader(file)
}

/// Load the store from a DataSource (file or URL) asynchronously.
///
/// For file sources, this performs synchronous file I/O.
/// For URL sources, this uses async HTTP requests.
pub async fn load_from_source(source: &DataSource) -> Result<(ActiveStore, SourceMetadata)> {
    match source {
        DataSource::File(path) => {
            let metadata = SourceMetadata::from_file(path)?;
            let entries = load_entries_from_file(path)?;
            let store = occlusion::build_store(entries)?;
            Ok((store, metadata))
        }
        DataSource::Url(url) => load_from_url(url).await,
    }
}

/// Check if the source has changed since the given metadata.
///
/// For files, checks the modification time (synchronous).
/// For URLs, does an async HEAD request to check ETag/Last-Modified.
pub async fn check_source_changed(
    source: &DataSource,
    old_metadata: &SourceMetadata,
) -> Result<bool> {
    match source {
        DataSource::File(path) => {
            let new_metadata = SourceMetadata::from_file(path)?;
            Ok(old_metadata.has_changed(&new_metadata))
        }
        DataSource::Url(url) => check_url_changed(url, old_metadata).await,
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

/// Load from a URL asynchronously.
async fn load_from_url(url: &str) -> Result<(ActiveStore, SourceMetadata)> {
    let response = reqwest::get(url).await?;

    if !response.status().is_success() {
        return Err(LoadError::HttpError(format!(
            "HTTP request failed with status: {}",
            response.status()
        )));
    }

    let metadata = extract_metadata_from_headers(response.headers());

    let text = response.text().await?;

    let entries = load_entries_from_reader(Cursor::new(text))?;
    let store = occlusion::build_store(entries)?;

    Ok((store, metadata))
}

/// Check if a URL has changed using HEAD request (async version).
async fn check_url_changed(url: &str, old_metadata: &SourceMetadata) -> Result<bool> {
    let client = reqwest::Client::new();
    let response = client.head(url).send().await?;

    if !response.status().is_success() {
        return Ok(true);
    }

    let new_metadata = extract_metadata_from_headers(response.headers());
    Ok(old_metadata.has_changed(&new_metadata))
}
