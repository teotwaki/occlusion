use crate::ActiveStore;
use crate::error::{Result, StoreError};

#[cfg(any(
    feature = "bench",
    not(any(feature = "vec", feature = "hybrid", feature = "fullhash"))
))]
use crate::store_hashmap::HashMapStore;
use csv::ReaderBuilder;
use serde::Deserialize;
use std::path::Path;
use uuid::Uuid;

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

/// Load entries from a CSV file.
///
/// Common helper that parses the CSV and returns a vector of (UUID, visibility) pairs.
fn load_entries<P: AsRef<Path>>(path: P) -> Result<Vec<(Uuid, u8)>> {
    let mut reader = ReaderBuilder::new().has_headers(true).from_path(path)?;

    let mut entries = Vec::new();

    for (line_num, result) in reader.deserialize().enumerate() {
        let record: CsvRecord = result?;

        let uuid = record
            .uuid
            .parse::<Uuid>()
            .map_err(|e| StoreError::InvalidFormat(format!("Line {}: {}", line_num + 2, e)))?;

        entries.push((uuid, record.visibility_level));
    }

    Ok(entries)
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
