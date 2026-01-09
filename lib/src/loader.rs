use crate::error::{Result, StoreError};
use crate::store_fullhash::FullHashStore;
use crate::store_hashmap::HashMapStore;
use crate::store_hybrid::HybridAuthStore;
use crate::store_vecstore::VecStore;
use csv::ReaderBuilder;
use serde::Deserialize;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct CsvRecord {
    uuid: String,
    visibility_level: u8,
}

/// Load the default store (HashMapStore) from a CSV file.
///
/// This is a convenience alias for `load_hashmap_from_csv`, which loads
/// the recommended default implementation.
///
/// # Example CSV format
/// ```csv
/// uuid,visibility_level
/// 550e8400-e29b-41d4-a716-446655440000,8
/// 6ba7b810-9dad-11d1-80b4-00c04fd430c8,15
/// ```
///
/// # Performance
/// See `load_hashmap_from_csv` for details.
pub fn load_default_from_csv<P: AsRef<Path>>(path: P) -> Result<HashMapStore> {
    load_hashmap_from_csv(path)
}

/// Load a VecStore (sorted vector) from a CSV file.
///
/// **Note**: For most applications, prefer `load_hashmap_from_csv` or
/// `load_default_from_csv` which are 4x faster.
///
/// The CSV file must have a header row with columns: `uuid,visibility_level`
///
/// # Example CSV format
/// ```csv
/// uuid,visibility_level
/// 550e8400-e29b-41d4-a716-446655440000,8
/// 6ba7b810-9dad-11d1-80b4-00c04fd430c8,15
/// ```
///
/// # When to Use
/// Use this when memory efficiency is critical. For most applications,
/// prefer `load_hashmap_from_csv` which is 4x faster.
///
/// # Errors
/// Returns an error if:
/// - The file cannot be read
/// - The CSV format is invalid
/// - Any UUID cannot be parsed
/// - Duplicate UUIDs are found
///
/// # Performance
/// - Loading: < 1 second for 1M UUIDs (dominated by UUID parsing)
/// - Lookups: ~51ns (O(log n) binary search)
/// - Memory: ~17 bytes/UUID (most efficient)
pub fn load_from_csv<P: AsRef<Path>>(path: P) -> Result<VecStore> {
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)?;

    let mut entries = Vec::new();

    for (line_num, result) in reader.deserialize().enumerate() {
        let record: CsvRecord = result?;

        // Parse UUID
        let uuid = record
            .uuid
            .parse::<Uuid>()
            .map_err(|e| StoreError::InvalidFormat(format!("Line {}: {}", line_num + 2, e)))?;

        entries.push((uuid, record.visibility_level));
    }

    // Create store (this will sort and check for duplicates)
    VecStore::new(entries)
        .map_err(|e| StoreError::InvalidFormat(e))
}

/// Load a HybridAuthStore from a CSV file.
///
/// This function loads the same CSV format as `load_from_csv`, but creates
/// a HybridAuthStore optimized for skewed distributions where most UUIDs
/// have visibility level 0.
///
/// # Example CSV format
/// ```csv
/// uuid,visibility_level
/// 550e8400-e29b-41d4-a716-446655440000,0
/// 6ba7b810-9dad-11d1-80b4-00c04fd430c8,15
/// ```
///
/// # When to Use
/// Use this when you have a known skewed distribution (80-90% at level 0)
/// and want optimized performance for that common case. For unknown distributions,
/// prefer `load_hashmap_from_csv`.
///
/// # Performance
/// - Best suited for distributions where 80-90% of UUIDs have visibility 0
/// - Level 0 lookups: ~12ns (competitive with HashMap)
/// - Higher level lookups: ~58ns
/// - Provides ~4x faster average lookups compared to binary search on skewed data
pub fn load_hybrid_from_csv<P: AsRef<Path>>(path: P) -> Result<HybridAuthStore> {
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)?;

    let mut entries = Vec::new();

    for (line_num, result) in reader.deserialize().enumerate() {
        let record: CsvRecord = result?;

        // Parse UUID
        let uuid = record
            .uuid
            .parse::<Uuid>()
            .map_err(|e| StoreError::InvalidFormat(format!("Line {}: {}", line_num + 2, e)))?;

        entries.push((uuid, record.visibility_level));
    }

    // Create hybrid store (this will partition, sort, and check for duplicates)
    HybridAuthStore::new(entries)
        .map_err(|e| StoreError::InvalidFormat(e))
}

/// Load a FullHashStore from a CSV file.
///
/// This function loads the same CSV format as `load_from_csv`, but creates
/// a FullHashStore that uses one HashSet per visibility level.
///
/// # Example CSV format
/// ```csv
/// uuid,visibility_level
/// 550e8400-e29b-41d4-a716-446655440000,0
/// 6ba7b810-9dad-11d1-80b4-00c04fd430c8,15
/// ```
///
/// # When to Use
/// Use this when you need to optimize worst-case scenarios (mask=0 queries).
/// For general use, prefer `load_hashmap_from_csv`.
///
/// # Performance
/// - O(1) lookups with early exit optimization
/// - Worst case (mask=0): ~11ns (best of all implementations)
/// - Higher levels: ~71ns (checks multiple levels)
/// - Higher memory overhead (256 HashSets)
pub fn load_fullhash_from_csv<P: AsRef<Path>>(path: P) -> Result<FullHashStore> {
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)?;

    let mut entries = Vec::new();

    for (line_num, result) in reader.deserialize().enumerate() {
        let record: CsvRecord = result?;

        // Parse UUID
        let uuid = record
            .uuid
            .parse::<Uuid>()
            .map_err(|e| StoreError::InvalidFormat(format!("Line {}: {}", line_num + 2, e)))?;

        entries.push((uuid, record.visibility_level));
    }

    // Create full hash store (this will partition into HashSets and check for duplicates)
    FullHashStore::new(entries)
        .map_err(|e| StoreError::InvalidFormat(e))
}

/// Load a HashMapStore from a CSV file (RECOMMENDED DEFAULT).
///
/// This function loads the same CSV format as `load_from_csv`, but creates
/// a HashMapStore that uses a simple `HashMap<Uuid, u8>` for O(1) lookups.
///
/// **This is the recommended default** for most applications due to its
/// simplicity, speed, and consistent performance.
///
/// # Example CSV format
/// ```csv
/// uuid,visibility_level
/// 550e8400-e29b-41d4-a716-446655440000,0
/// 6ba7b810-9dad-11d1-80b4-00c04fd430c8,15
/// ```
///
/// # Performance
/// - Fastest implementation in almost all scenarios
/// - Consistent ~13ns lookups regardless of visibility level
/// - Batch queries: ~1.33µs for 100 UUIDs (fastest)
/// - 4-6x faster than sorted vector implementation
/// - Competitive with specialized implementations even on skewed workloads
pub fn load_hashmap_from_csv<P: AsRef<Path>>(path: P) -> Result<HashMapStore> {
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)?;

    let mut entries = Vec::new();

    for (line_num, result) in reader.deserialize().enumerate() {
        let record: CsvRecord = result?;

        // Parse UUID
        let uuid = record
            .uuid
            .parse::<Uuid>()
            .map_err(|e| StoreError::InvalidFormat(format!("Line {}: {}", line_num + 2, e)))?;

        entries.push((uuid, record.visibility_level));
    }

    // Create hashmap store (this will check for duplicates)
    HashMapStore::new(entries)
        .map_err(|e| StoreError::InvalidFormat(e))
}

#[cfg(test)]
mod tests {
    use super::*;
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
