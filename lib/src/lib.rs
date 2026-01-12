//! # Occlusion Store
//!
//! A high-performance authorization store for managing UUID visibility levels.
//!
//! ## Overview
//!
//! This library provides efficient data structures for storing and querying
//! millions of UUIDs with associated visibility levels (0-255). The visibility
//! model is hierarchical: a request with visibility mask M can see all UUIDs
//! with visibility level L where L <= M.
//!
//! ## Store Implementation
//!
//! The active store implementation is selected at compile time via feature flags:
//!
//! - **Default (no feature)**: `HashMapStore` - O(1) lookups, ~2.7ns with FxHash
//! - **`--features vec`**: `VecStore` - O(log n) lookups, ~51ns, lowest memory
//! - **`--features hybrid`**: `HybridAuthStore` - Optimized for skewed distributions
//! - **`--features fullhash`**: `FullHashStore` - 256 HashSets, best worst-case
//!
//! ## Performance (with FxHash, 2M UUIDs)
//!
//! | Implementation | Lookup | Batch (100) | Memory |
//! |----------------|--------|-------------|--------|
//! | HashMapStore | 2.7ns | 347ns | ~24-32 bytes/UUID |
//! | VecStore | 51ns | 7.9µs | ~17 bytes/UUID |
//! | HybridAuthStore | 2.5-48ns | 780ns | ~24 bytes/UUID |
//! | FullHashStore | 2.3-21ns | 422ns | Highest |
//!
//! ## Feature Flags
//!
//! - `nofx`: Use std HashMap instead of FxHash (slower but no extra dependency)
//! - `vec`: Use VecStore (sorted vector with binary search)
//! - `hybrid`: Use HybridAuthStore (HashSet for level 0 + sorted vector)
//! - `fullhash`: Use FullHashStore (256 HashSets, one per level)
//! - `bench`: Enable all stores for benchmark comparisons
//!
//! ## Thread Safety
//!
//! All store implementations are immutable after construction and implement `Send + Sync`,
//! making them safe to share across threads (e.g., wrapped in `Arc`).
//!
//! ## Example
//!
//! ```ignore
//! use occlusion::{build_store, Store};
//! use uuid::Uuid;
//!
//! // Build store from entries
//! let entries = vec![
//!     (Uuid::new_v4(), 0),   // Level 0 - visible to all
//!     (Uuid::new_v4(), 10),  // Level 10
//! ];
//! let store = build_store(entries)?;
//!
//! // Check single UUID
//! let uuid = "550e8400-e29b-41d4-a716-446655440000".parse()?;
//! if store.is_visible(&uuid, 10) {
//!     println!("UUID is visible at level 10");
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::doc_markdown)] // Allow type names without backticks in docs
#![allow(clippy::cast_precision_loss)] // Acceptable for percentage calculations
#![allow(clippy::missing_errors_doc)] // Error conditions are self-evident
#![allow(clippy::missing_panics_doc)] // Panics are from RwLock poisoning which is unrecoverable

mod error;

// Store modules - conditionally compiled based on features
// HashMapStore is always available (default)
mod store_hashmap;

// Alternative stores - only compiled when their feature or bench is enabled
#[cfg(any(feature = "bench", feature = "vec"))]
mod store_vecstore;

#[cfg(any(feature = "bench", feature = "hybrid"))]
mod store_hybrid;

#[cfg(any(feature = "bench", feature = "fullhash"))]
mod store_fullhash;

// Re-exports
pub use error::{Result, StoreError};
pub use store_hashmap::HashMapStore;

/// Statistics about the distribution of UUIDs across visibility levels.
///
/// This struct provides insight into how UUIDs are distributed, which can help
/// determine if specialized store implementations (like `HybridAuthStore`) would
/// be beneficial for your workload.
#[derive(Debug, Clone)]
#[must_use]
pub struct DistributionStats {
    /// Total number of UUIDs in the store
    pub total_uuids: usize,
    /// Number of UUIDs at visibility level 0
    pub level_0_count: usize,
    /// Number of UUIDs at visibility levels 1-255
    pub higher_levels_count: usize,
    /// Percentage of UUIDs at level 0 (0.0 to 100.0)
    pub level_0_percentage: f64,
}

impl std::fmt::Display for DistributionStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Total: {}, Level 0: {} ({:.1}%), Higher: {}",
            self.total_uuids, self.level_0_count, self.level_0_percentage, self.higher_levels_count
        )
    }
}

// Conditional re-exports for bench mode
#[cfg(any(feature = "bench", feature = "vec"))]
pub use store_vecstore::VecStore;

#[cfg(any(feature = "bench", feature = "hybrid"))]
pub use store_hybrid::HybridAuthStore;

#[cfg(any(feature = "bench", feature = "fullhash"))]
pub use store_fullhash::FullHashStore;

// HashMap type alias based on nofx feature
#[cfg(not(feature = "nofx"))]
pub use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

#[cfg(feature = "nofx")]
pub use std::collections::{HashMap, HashSet};

use uuid::Uuid;

/// Common trait for all store implementations.
///
/// This allows the server to be generic over store type.
pub trait Store: Send + Sync {
    /// Check if a UUID is visible at the given mask level.
    #[must_use]
    fn is_visible(&self, uuid: &Uuid, mask: u8) -> bool;

    /// Check if all UUIDs in the batch are visible at the given mask level.
    #[must_use]
    fn check_batch(&self, uuids: &[Uuid], mask: u8) -> bool;

    /// Return the number of UUIDs in the store.
    #[must_use]
    fn len(&self) -> usize;

    /// Return true if the store contains no UUIDs.
    #[must_use]
    fn is_empty(&self) -> bool;

    /// Return a map of visibility level to count of UUIDs at that level.
    #[must_use]
    fn visibility_distribution(&self) -> HashMap<u8, usize>;
}

/// Type alias for the active store implementation.
///
/// Selected at compile time based on feature flags:
/// - Default: `HashMapStore`
/// - `--features vec`: `VecStore`
/// - `--features hybrid`: `HybridAuthStore`
/// - `--features fullhash`: `FullHashStore`
#[cfg(feature = "fullhash")]
pub type ActiveStore = FullHashStore;

#[cfg(all(feature = "hybrid", not(feature = "fullhash")))]
pub type ActiveStore = HybridAuthStore;

#[cfg(all(feature = "vec", not(feature = "hybrid"), not(feature = "fullhash")))]
pub type ActiveStore = VecStore;

#[cfg(not(any(feature = "vec", feature = "hybrid", feature = "fullhash")))]
pub type ActiveStore = HashMapStore;

/// Build an ActiveStore from a vector of (UUID, visibility_level) pairs.
///
/// The store implementation is selected at compile time based on feature flags.
#[cfg(feature = "fullhash")]
pub fn build_store(entries: Vec<(Uuid, u8)>) -> Result<ActiveStore> {
    FullHashStore::new(entries)
}

#[cfg(all(feature = "hybrid", not(feature = "fullhash")))]
pub fn build_store(entries: Vec<(Uuid, u8)>) -> Result<ActiveStore> {
    HybridAuthStore::new(entries)
}

#[cfg(all(feature = "vec", not(feature = "hybrid"), not(feature = "fullhash")))]
pub fn build_store(entries: Vec<(Uuid, u8)>) -> Result<ActiveStore> {
    VecStore::new(entries)
}

#[cfg(not(any(feature = "vec", feature = "hybrid", feature = "fullhash")))]
pub fn build_store(entries: Vec<(Uuid, u8)>) -> Result<ActiveStore> {
    HashMapStore::new(entries)
}

// Swappable store for runtime reloading
mod swappable;
pub use swappable::SwappableStore;

// Bench-only store builders for benchmark comparisons

/// Build a `HashMapStore` for benchmark comparisons.
#[cfg(feature = "bench")]
pub fn build_hashmap_store(entries: Vec<(Uuid, u8)>) -> Result<HashMapStore> {
    HashMapStore::new(entries)
}

/// Build a `VecStore` for benchmark comparisons.
#[cfg(feature = "bench")]
pub fn build_vec_store(entries: Vec<(Uuid, u8)>) -> Result<VecStore> {
    VecStore::new(entries)
}

/// Build a `HybridAuthStore` for benchmark comparisons.
#[cfg(feature = "bench")]
pub fn build_hybrid_store(entries: Vec<(Uuid, u8)>) -> Result<HybridAuthStore> {
    HybridAuthStore::new(entries)
}

/// Build a `FullHashStore` for benchmark comparisons.
#[cfg(feature = "bench")]
pub fn build_fullhash_store(entries: Vec<(Uuid, u8)>) -> Result<FullHashStore> {
    FullHashStore::new(entries)
}

#[cfg(all(test, feature = "bench"))]
mod store_tests {
    //! Parameterized tests that run against all store implementations.
    //!
    //! These tests use rstest to ensure consistent behavior across all stores.
    //! Enable with: `cargo test --features bench`

    use super::*;
    use rstest::rstest;

    /// Test data: entries with various visibility levels
    fn test_entries() -> Vec<(Uuid, u8)> {
        vec![
            (Uuid::from_u128(1), 0),   // Level 0 - visible to all
            (Uuid::from_u128(2), 5),   // Level 5
            (Uuid::from_u128(3), 10),  // Level 10
            (Uuid::from_u128(4), 255), // Level 255 - most restricted
        ]
    }

    // Store factory functions for rstest
    fn hashmap_store() -> Box<dyn Store> {
        Box::new(HashMapStore::new(test_entries()).unwrap())
    }

    fn vec_store() -> Box<dyn Store> {
        Box::new(VecStore::new(test_entries()).unwrap())
    }

    fn hybrid_store() -> Box<dyn Store> {
        Box::new(HybridAuthStore::new(test_entries()).unwrap())
    }

    fn fullhash_store() -> Box<dyn Store> {
        Box::new(FullHashStore::new(test_entries()).unwrap())
    }

    #[rstest]
    #[case::hashmap(hashmap_store())]
    #[case::vec(vec_store())]
    #[case::hybrid(hybrid_store())]
    #[case::fullhash(fullhash_store())]
    fn test_is_visible_level_0(#[case] store: Box<dyn Store>) {
        let uuid = Uuid::from_u128(1);
        // Level 0 UUID should be visible at any mask
        assert!(store.is_visible(&uuid, 0));
        assert!(store.is_visible(&uuid, 128));
        assert!(store.is_visible(&uuid, 255));
    }

    #[rstest]
    #[case::hashmap(hashmap_store())]
    #[case::vec(vec_store())]
    #[case::hybrid(hybrid_store())]
    #[case::fullhash(fullhash_store())]
    fn test_is_visible_higher_level(#[case] store: Box<dyn Store>) {
        let uuid = Uuid::from_u128(2); // Level 5
        assert!(!store.is_visible(&uuid, 0)); // 5 > 0
        assert!(!store.is_visible(&uuid, 4)); // 5 > 4
        assert!(store.is_visible(&uuid, 5)); // 5 <= 5
        assert!(store.is_visible(&uuid, 10)); // 5 <= 10
    }

    #[rstest]
    #[case::hashmap(hashmap_store())]
    #[case::vec(vec_store())]
    #[case::hybrid(hybrid_store())]
    #[case::fullhash(fullhash_store())]
    fn test_is_visible_unknown_uuid(#[case] store: Box<dyn Store>) {
        let unknown = Uuid::from_u128(999);
        // Unknown UUIDs should never be visible
        assert!(!store.is_visible(&unknown, 0));
        assert!(!store.is_visible(&unknown, 255));
    }

    #[rstest]
    #[case::hashmap(hashmap_store())]
    #[case::vec(vec_store())]
    #[case::hybrid(hybrid_store())]
    #[case::fullhash(fullhash_store())]
    fn test_check_batch_all_visible(#[case] store: Box<dyn Store>) {
        let uuids = vec![Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)];
        // All should be visible at mask 10 (levels 0, 5, 10)
        assert!(store.check_batch(&uuids, 10));
    }

    #[rstest]
    #[case::hashmap(hashmap_store())]
    #[case::vec(vec_store())]
    #[case::hybrid(hybrid_store())]
    #[case::fullhash(fullhash_store())]
    fn test_check_batch_partial_visible(#[case] store: Box<dyn Store>) {
        let uuids = vec![Uuid::from_u128(1), Uuid::from_u128(4)]; // Levels 0, 255
        // Not all visible at mask 254
        assert!(!store.check_batch(&uuids, 254));
        // All visible at mask 255
        assert!(store.check_batch(&uuids, 255));
    }

    #[rstest]
    #[case::hashmap(hashmap_store())]
    #[case::vec(vec_store())]
    #[case::hybrid(hybrid_store())]
    #[case::fullhash(fullhash_store())]
    fn test_len_and_is_empty(#[case] store: Box<dyn Store>) {
        assert_eq!(store.len(), 4);
        assert!(!store.is_empty());
    }

    #[rstest]
    #[case::hashmap(hashmap_store())]
    #[case::vec(vec_store())]
    #[case::hybrid(hybrid_store())]
    #[case::fullhash(fullhash_store())]
    fn test_visibility_distribution(#[case] store: Box<dyn Store>) {
        let dist = store.visibility_distribution();
        assert_eq!(dist.get(&0), Some(&1));
        assert_eq!(dist.get(&5), Some(&1));
        assert_eq!(dist.get(&10), Some(&1));
        assert_eq!(dist.get(&255), Some(&1));
        assert_eq!(dist.get(&100), None); // No entries at level 100
    }

    #[rstest]
    #[case::hashmap(hashmap_store())]
    #[case::vec(vec_store())]
    #[case::hybrid(hybrid_store())]
    #[case::fullhash(fullhash_store())]
    fn test_empty_batch_returns_true(#[case] store: Box<dyn Store>) {
        // Empty batch should return true (vacuous truth)
        assert!(store.check_batch(&[], 0));
    }
}
