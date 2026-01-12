#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_precision_loss)]

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
//! - **Default (no feature)**: `HashMapStore` - O(1) lookups, ~2.7ns with `FxHash`
//! - **`--features vec`**: `VecStore` - O(log n) lookups, ~51ns, lowest memory
//! - **`--features hybrid`**: `HybridAuthStore` - Optimized for skewed distributions
//! - **`--features fullhash`**: `FullHashStore` - 256 `HashSets`, best worst-case
//!
//! ## Performance (with `FxHash`, 2M UUIDs)
//!
//! | Implementation | Lookup | Batch (100) | Memory |
//! |----------------|--------|-------------|--------|
//! | `HashMapStore` | 2.7ns | 347ns | ~24-32 bytes/UUID |
//! | `VecStore` | 51ns | 7.9µs | ~17 bytes/UUID |
//! | `HybridAuthStore` | 2.5-48ns | 780ns | ~24 bytes/UUID |
//! | `FullHashStore` | 2.3-21ns | 422ns | Highest |
//!
//! ## Feature Flags
//!
//! - `nofx`: Use std `HashMap` instead of `FxHash` (slower but no extra dependency)
//! - `vec`: Use `VecStore` (sorted vector with binary search)
//! - `hybrid`: Use `HybridAuthStore` (`HashSet` for level 0 + sorted vector)
//! - `fullhash`: Use `FullHashStore` (256 `HashSets`, one per level)
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
    fn is_visible(&self, uuid: &Uuid, mask: u8) -> bool;
    fn check_batch(&self, uuids: &[Uuid], mask: u8) -> bool;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn visibility_distribution(&self) -> HashMap<u8, usize>;
}

/// Statistics about the distribution of UUIDs across visibility levels.
#[derive(Debug, Clone)]
pub struct DistributionStats {
    pub total_uuids: usize,
    pub level_0_count: usize,
    pub higher_levels_count: usize,
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

/// Build an `ActiveStore` from a vector of (UUID, `visibility_level`) pairs.
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
#[cfg(feature = "bench")]
pub fn build_hashmap_store(entries: Vec<(Uuid, u8)>) -> Result<HashMapStore> {
    HashMapStore::new(entries)
}

#[cfg(feature = "bench")]
pub fn build_vec_store(entries: Vec<(Uuid, u8)>) -> Result<VecStore> {
    VecStore::new(entries)
}

#[cfg(feature = "bench")]
pub fn build_hybrid_store(entries: Vec<(Uuid, u8)>) -> Result<HybridAuthStore> {
    HybridAuthStore::new(entries)
}

#[cfg(feature = "bench")]
pub fn build_fullhash_store(entries: Vec<(Uuid, u8)>) -> Result<FullHashStore> {
    FullHashStore::new(entries)
}

/// Parameterized tests that run against all store implementations.
///
/// These tests ensure consistent behavior across all store types.
/// Only compiled with the `bench` feature to access all store implementations.
#[cfg(all(test, feature = "bench"))]
mod parameterized_tests {
    use super::*;
    use rstest::rstest;

    /// Helper to create a boxed store from entries using a builder function.
    fn make_store<S: Store + 'static>(
        entries: Vec<(Uuid, u8)>,
        builder: fn(Vec<(Uuid, u8)>) -> Result<S>,
    ) -> Box<dyn Store> {
        Box::new(builder(entries).unwrap())
    }

    #[rstest]
    #[case::hashmap(build_hashmap_store as fn(Vec<(Uuid, u8)>) -> Result<HashMapStore>)]
    #[case::vec(build_vec_store as fn(Vec<(Uuid, u8)>) -> Result<VecStore>)]
    #[case::hybrid(build_hybrid_store as fn(Vec<(Uuid, u8)>) -> Result<HybridAuthStore>)]
    #[case::fullhash(build_fullhash_store as fn(Vec<(Uuid, u8)>) -> Result<FullHashStore>)]
    fn test_is_visible_level_0<S: Store + 'static>(
        #[case] builder: fn(Vec<(Uuid, u8)>) -> Result<S>,
    ) {
        let uuid = Uuid::from_u128(1);
        let entries = vec![(uuid, 0)];
        let store = make_store(entries, builder);

        // Level 0 is visible at all masks
        assert!(store.is_visible(&uuid, 0));
        assert!(store.is_visible(&uuid, 10));
        assert!(store.is_visible(&uuid, 255));
    }

    #[rstest]
    #[case::hashmap(build_hashmap_store as fn(Vec<(Uuid, u8)>) -> Result<HashMapStore>)]
    #[case::vec(build_vec_store as fn(Vec<(Uuid, u8)>) -> Result<VecStore>)]
    #[case::hybrid(build_hybrid_store as fn(Vec<(Uuid, u8)>) -> Result<HybridAuthStore>)]
    #[case::fullhash(build_fullhash_store as fn(Vec<(Uuid, u8)>) -> Result<FullHashStore>)]
    fn test_is_visible_higher_levels<S: Store + 'static>(
        #[case] builder: fn(Vec<(Uuid, u8)>) -> Result<S>,
    ) {
        let uuid = Uuid::from_u128(1);
        let entries = vec![(uuid, 8)];
        let store = make_store(entries, builder);

        assert!(store.is_visible(&uuid, 10)); // 8 <= 10
        assert!(store.is_visible(&uuid, 8)); // 8 <= 8
        assert!(!store.is_visible(&uuid, 7)); // 8 > 7
        assert!(!store.is_visible(&uuid, 0)); // 8 > 0
    }

    #[rstest]
    #[case::hashmap(build_hashmap_store as fn(Vec<(Uuid, u8)>) -> Result<HashMapStore>)]
    #[case::vec(build_vec_store as fn(Vec<(Uuid, u8)>) -> Result<VecStore>)]
    #[case::hybrid(build_hybrid_store as fn(Vec<(Uuid, u8)>) -> Result<HybridAuthStore>)]
    #[case::fullhash(build_fullhash_store as fn(Vec<(Uuid, u8)>) -> Result<FullHashStore>)]
    fn test_is_visible_missing_uuid<S: Store + 'static>(
        #[case] builder: fn(Vec<(Uuid, u8)>) -> Result<S>,
    ) {
        let uuid = Uuid::from_u128(999);
        let entries = vec![(Uuid::from_u128(1), 0)];
        let store = make_store(entries, builder);

        assert!(!store.is_visible(&uuid, 255));
    }

    #[rstest]
    #[case::hashmap(build_hashmap_store as fn(Vec<(Uuid, u8)>) -> Result<HashMapStore>)]
    #[case::vec(build_vec_store as fn(Vec<(Uuid, u8)>) -> Result<VecStore>)]
    #[case::hybrid(build_hybrid_store as fn(Vec<(Uuid, u8)>) -> Result<HybridAuthStore>)]
    #[case::fullhash(build_fullhash_store as fn(Vec<(Uuid, u8)>) -> Result<FullHashStore>)]
    fn test_check_batch<S: Store + 'static>(#[case] builder: fn(Vec<(Uuid, u8)>) -> Result<S>) {
        let uuid1 = Uuid::from_u128(1);
        let uuid2 = Uuid::from_u128(2);
        let uuid3 = Uuid::from_u128(3);

        let entries = vec![(uuid1, 0), (uuid2, 10), (uuid3, 15)];
        let store = make_store(entries, builder);

        // All visible at mask 15
        assert!(store.check_batch(&[uuid1, uuid2, uuid3], 15));
        // Not all visible at mask 10 (uuid3 has level 15)
        assert!(!store.check_batch(&[uuid1, uuid2, uuid3], 10));
        // Subset that is all visible
        assert!(store.check_batch(&[uuid1, uuid2], 10));
    }

    #[rstest]
    #[case::hashmap(build_hashmap_store as fn(Vec<(Uuid, u8)>) -> Result<HashMapStore>)]
    #[case::vec(build_vec_store as fn(Vec<(Uuid, u8)>) -> Result<VecStore>)]
    #[case::hybrid(build_hybrid_store as fn(Vec<(Uuid, u8)>) -> Result<HybridAuthStore>)]
    #[case::fullhash(build_fullhash_store as fn(Vec<(Uuid, u8)>) -> Result<FullHashStore>)]
    fn test_len_and_is_empty<S: Store + 'static>(
        #[case] builder: fn(Vec<(Uuid, u8)>) -> Result<S>,
    ) {
        let empty_store = make_store(vec![], builder);
        assert!(empty_store.is_empty());
        assert_eq!(empty_store.len(), 0);

        let store = make_store(vec![(Uuid::from_u128(1), 5)], builder);
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[rstest]
    #[case::hashmap(build_hashmap_store as fn(Vec<(Uuid, u8)>) -> Result<HashMapStore>)]
    #[case::vec(build_vec_store as fn(Vec<(Uuid, u8)>) -> Result<VecStore>)]
    #[case::hybrid(build_hybrid_store as fn(Vec<(Uuid, u8)>) -> Result<HybridAuthStore>)]
    #[case::fullhash(build_fullhash_store as fn(Vec<(Uuid, u8)>) -> Result<FullHashStore>)]
    fn test_visibility_distribution<S: Store + 'static>(
        #[case] builder: fn(Vec<(Uuid, u8)>) -> Result<S>,
    ) {
        let entries = vec![
            (Uuid::from_u128(1), 5),
            (Uuid::from_u128(2), 5),
            (Uuid::from_u128(3), 10),
            (Uuid::from_u128(4), 5),
        ];
        let store = make_store(entries, builder);

        let dist = store.visibility_distribution();
        assert_eq!(dist.get(&5), Some(&3));
        assert_eq!(dist.get(&10), Some(&1));
        assert_eq!(dist.get(&15), None);
    }

    #[rstest]
    #[case::hashmap(HashMapStore::new)]
    #[case::vec(VecStore::new)]
    #[case::hybrid(HybridAuthStore::new)]
    #[case::fullhash(FullHashStore::new)]
    fn test_duplicate_detection<S: Store + 'static>(
        #[case] builder: fn(Vec<(Uuid, u8)>) -> Result<S>,
    ) {
        let uuid = Uuid::from_u128(42);
        let entries = vec![(uuid, 0), (uuid, 5)];
        assert!(builder(entries).is_err());
    }
}
