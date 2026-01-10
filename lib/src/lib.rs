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
//! use occlusion::{load_from_csv, Store};
//!
//! // Load from CSV file (uses compile-time selected implementation)
//! let store = load_from_csv("data.csv")?;
//!
//! // Check single UUID
//! let uuid = "550e8400-e29b-41d4-a716-446655440000".parse()?;
//! if store.is_visible(&uuid, 10) {
//!     println!("UUID is visible at level 10");
//! }
//!
//! // Batch check
//! let uuids = vec![uuid1, uuid2, uuid3];
//! let results = store.check_batch(&uuids, 10);
//! ```

mod error;
mod loader;

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
pub use loader::load_from_csv;
pub use store_hashmap::HashMapStore;

// Bench-only exports for benchmark comparisons
#[cfg(feature = "bench")]
pub use loader::{
    load_fullhash_from_csv, load_hashmap_from_csv, load_hybrid_from_csv, load_vec_from_csv,
};

// Conditional re-exports for bench mode
#[cfg(any(feature = "bench", feature = "vec"))]
pub use store_vecstore::VecStore;

#[cfg(any(feature = "bench", feature = "hybrid"))]
pub use store_hybrid::{DistributionStats, HybridAuthStore};

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
    fn get_visibility(&self, uuid: &Uuid) -> Option<u8>;
    fn is_visible(&self, uuid: &Uuid, mask: u8) -> bool;
    fn check_batch(&self, uuids: &[Uuid], mask: u8) -> Vec<bool>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
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
