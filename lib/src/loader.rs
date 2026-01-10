use crate::ActiveStore;
use crate::error::{Result, StoreError};
use crate::source::{DataSource, SourceMetadata};

#[cfg(any(
    feature = "bench",
    not(any(feature = "vec", feature = "hybrid", feature = "fullhash"))
))]
use crate::store_hashmap::HashMapStore;
use csv::ReaderBuilder;
use serde::Deserialize;
use std::io::Read;
use std::path::Path;
use uuid::Uuid;

#[cfg(feature = "url")]
use std::io::Cursor;

#[cfg(any(feature = "bench", feature = "fullhash"))]
use crate::store_fullhash::FullHashStore;

#[cfg(any(feature = "bench", feature = "hybrid"))]
use crate::store_hybrid::HybridAuthStore;

#[cfg(any(feature = "bench", feature = "vec"))]
use crate::store_vecstore::VecStore;

#[derive(Debug, Deserialize)]
struct CsvRecord {
    uuid: String,
    visibility_level: u8,
}

/// Core CSV parsing from any reader.
///
/// This is the shared implementation used by all loaders (file, URL, string).
fn load_entries_from_reader<R: Read>(reader: R) -> Result<Vec<(Uuid, u8)>> {
    let mut csv_reader = ReaderBuilder::new().has_headers(true).from_reader(reader);

    let mut entries = Vec::new();

    for (line_num, result) in csv_reader.deserialize().enumerate() {
        let record: CsvRecord = result?;

        let uuid = record
            .uuid
            .parse::<Uuid>()
            .map_err(|e| StoreError::InvalidFormat(format!("Line {}: {}", line_num + 2, e)))?;

        entries.push((uuid, record.visibility_level));
    }

    Ok(entries)
}

/// Load entries from a CSV file.
fn load_entries<P: AsRef<Path>>(path: P) -> Result<Vec<(Uuid, u8)>> {
    let file =
        std::fs::File::open(path.as_ref()).map_err(|e| StoreError::IoError(e.to_string()))?;
    load_entries_from_reader(file)
}

/// Load the active store implementation from a CSV file.
///
/// The store implementation is selected at compile time based on feature flags:
/// - Default: `HashMapStore` (~2.7ns lookups with FxHash)
/// - `--features vec`: `VecStore` (~51ns lookups, lowest memory)
/// - `--features hybrid`: `HybridAuthStore` (optimized for skewed distributions)
/// - `--features fullhash`: `FullHashStore` (best worst-case performance)
///
/// # Example CSV format
/// ```csv
/// uuid,visibility_level
/// 550e8400-e29b-41d4-a716-446655440000,8
/// 6ba7b810-9dad-11d1-80b4-00c04fd430c8,15
/// ```
///
/// # Errors
/// Returns an error if:
/// - The file cannot be read
/// - The CSV format is invalid
/// - Any UUID cannot be parsed
/// - Duplicate UUIDs are found
#[cfg(feature = "fullhash")]
pub fn load_from_csv<P: AsRef<Path>>(path: P) -> Result<ActiveStore> {
    let entries = load_entries(path)?;
    FullHashStore::new(entries).map_err(StoreError::InvalidFormat)
}

#[cfg(all(feature = "hybrid", not(feature = "fullhash")))]
pub fn load_from_csv<P: AsRef<Path>>(path: P) -> Result<ActiveStore> {
    let entries = load_entries(path)?;
    HybridAuthStore::new(entries).map_err(StoreError::InvalidFormat)
}

#[cfg(all(feature = "vec", not(feature = "hybrid"), not(feature = "fullhash")))]
pub fn load_from_csv<P: AsRef<Path>>(path: P) -> Result<ActiveStore> {
    let entries = load_entries(path)?;
    VecStore::new(entries).map_err(StoreError::InvalidFormat)
}

#[cfg(not(any(feature = "vec", feature = "hybrid", feature = "fullhash")))]
pub fn load_from_csv<P: AsRef<Path>>(path: P) -> Result<ActiveStore> {
    let entries = load_entries(path)?;
    HashMapStore::new(entries).map_err(StoreError::InvalidFormat)
}

// ============================================================================
// Source-based loading (file or URL)
// ============================================================================

/// Build ActiveStore from entries (shared by all loaders).
#[cfg(feature = "fullhash")]
fn build_store(entries: Vec<(Uuid, u8)>) -> Result<ActiveStore> {
    FullHashStore::new(entries).map_err(StoreError::InvalidFormat)
}

#[cfg(all(feature = "hybrid", not(feature = "fullhash")))]
fn build_store(entries: Vec<(Uuid, u8)>) -> Result<ActiveStore> {
    HybridAuthStore::new(entries).map_err(StoreError::InvalidFormat)
}

#[cfg(all(feature = "vec", not(feature = "hybrid"), not(feature = "fullhash")))]
fn build_store(entries: Vec<(Uuid, u8)>) -> Result<ActiveStore> {
    VecStore::new(entries).map_err(StoreError::InvalidFormat)
}

#[cfg(not(any(feature = "vec", feature = "hybrid", feature = "fullhash")))]
fn build_store(entries: Vec<(Uuid, u8)>) -> Result<ActiveStore> {
    HashMapStore::new(entries).map_err(StoreError::InvalidFormat)
}

/// Load the store from a DataSource (file or URL).
///
/// Also returns the source metadata for conditional reloading.
///
/// # Example
///
/// ```ignore
/// use occlusion::{load_from_source, DataSource};
///
/// let source = DataSource::parse("data.csv");
/// let (store, metadata) = load_from_source(&source)?;
/// ```
pub fn load_from_source(source: &DataSource) -> Result<(ActiveStore, SourceMetadata)> {
    match source {
        DataSource::File(path) => {
            let metadata =
                SourceMetadata::from_file(path).map_err(|e| StoreError::IoError(e.to_string()))?;
            let entries = load_entries(path)?;
            let store = build_store(entries)?;
            Ok((store, metadata))
        }
        #[cfg(feature = "url")]
        DataSource::Url(url) => load_from_url(url),
    }
}

/// Check if the source has changed since the given metadata.
///
/// For files, checks the modification time.
/// For URLs, does a HEAD request to check ETag/Last-Modified.
pub fn check_source_changed(source: &DataSource, old_metadata: &SourceMetadata) -> Result<bool> {
    match source {
        DataSource::File(path) => {
            let new_metadata =
                SourceMetadata::from_file(path).map_err(|e| StoreError::IoError(e.to_string()))?;
            Ok(old_metadata.has_changed(&new_metadata))
        }
        #[cfg(feature = "url")]
        DataSource::Url(url) => check_url_changed(url, old_metadata),
    }
}

/// Extract metadata from HTTP response headers.
#[cfg(feature = "url")]
fn extract_metadata(response: &reqwest::blocking::Response) -> SourceMetadata {
    SourceMetadata {
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
    }
}

/// Load from a URL (only available with the "url" feature).
#[cfg(feature = "url")]
fn load_from_url(url: &str) -> Result<(ActiveStore, SourceMetadata)> {
    let response = reqwest::blocking::get(url)
        .map_err(|e| StoreError::IoError(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(StoreError::IoError(format!(
            "HTTP request failed with status: {}",
            response.status()
        )));
    }

    let metadata = extract_metadata(&response);

    let text = response
        .text()
        .map_err(|e| StoreError::IoError(format!("Failed to read response body: {}", e)))?;

    let entries = load_entries_from_reader(Cursor::new(text))?;
    let store = build_store(entries)?;

    Ok((store, metadata))
}

/// Check if a URL has changed using HEAD request.
#[cfg(feature = "url")]
fn check_url_changed(url: &str, old_metadata: &SourceMetadata) -> Result<bool> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .head(url)
        .send()
        .map_err(|e| StoreError::IoError(format!("HTTP HEAD request failed: {}", e)))?;

    if !response.status().is_success() {
        // If HEAD fails, assume changed
        return Ok(true);
    }

    let new_metadata = extract_metadata(&response);
    Ok(old_metadata.has_changed(&new_metadata))
}

// ============================================================================
// Individual store loaders (only available with bench feature for comparisons)
// ============================================================================

/// Load a HashMapStore from a CSV file.
///
/// Only available with the `bench` feature for benchmark comparisons.
#[cfg(feature = "bench")]
pub fn load_hashmap_from_csv<P: AsRef<Path>>(path: P) -> Result<HashMapStore> {
    let entries = load_entries(path)?;
    HashMapStore::new(entries).map_err(StoreError::InvalidFormat)
}

/// Load a VecStore from a CSV file.
///
/// Only available with the `bench` feature for benchmark comparisons.
#[cfg(feature = "bench")]
pub fn load_vec_from_csv<P: AsRef<Path>>(path: P) -> Result<VecStore> {
    let entries = load_entries(path)?;
    VecStore::new(entries).map_err(StoreError::InvalidFormat)
}

/// Load a HybridAuthStore from a CSV file.
///
/// Only available with the `bench` feature for benchmark comparisons.
#[cfg(feature = "bench")]
pub fn load_hybrid_from_csv<P: AsRef<Path>>(path: P) -> Result<HybridAuthStore> {
    let entries = load_entries(path)?;
    HybridAuthStore::new(entries).map_err(StoreError::InvalidFormat)
}

/// Load a FullHashStore from a CSV file.
///
/// Only available with the `bench` feature for benchmark comparisons.
#[cfg(feature = "bench")]
pub fn load_fullhash_from_csv<P: AsRef<Path>>(path: P) -> Result<FullHashStore> {
    let entries = load_entries(path)?;
    FullHashStore::new(entries).map_err(StoreError::InvalidFormat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_valid_csv() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "uuid,visibility_level").unwrap();
        writeln!(file, "550e8400-e29b-41d4-a716-446655440000,8").unwrap();
        writeln!(file, "6ba7b810-9dad-11d1-80b4-00c04fd430c8,15").unwrap();
        file.flush().unwrap();

        let store = load_from_csv(file.path()).unwrap();
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn test_load_invalid_uuid() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "uuid,visibility_level").unwrap();
        writeln!(file, "not-a-uuid,8").unwrap();
        file.flush().unwrap();

        assert!(load_from_csv(file.path()).is_err());
    }

    #[test]
    fn test_load_missing_file() {
        let result = load_from_csv("/nonexistent/path.csv");
        assert!(result.is_err());
    }
}
