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
//! ## Available Implementations
//!
//! Four different store implementations are provided, each optimized for different scenarios:
//!
//! ### 1. HashMapStore (Recommended Default)
//! - **Algorithm**: Simple `HashMap<Uuid, u8>`
//! - **Performance**: ~13ns per lookup (O(1))
//! - **Memory**: ~24-32 bytes per UUID
//! - **Best for**: All workloads, especially uniform distributions
//! - **Advantage**: Simplest, fastest in almost all scenarios (4-6x faster than sorted)
//!
//! ### 2. HybridAuthStore
//! - **Algorithm**: HashSet for level 0 + sorted array for levels 1-255
//! - **Performance**: ~12ns for level 0 (90% of queries), ~58ns for higher levels
//! - **Memory**: ~24 bytes per UUID
//! - **Best for**: Skewed distributions where 80-90% of UUIDs have visibility level 0
//! - **Advantage**: Optimized hot path for level 0, lower memory than FullHashStore
//!
//! ### 3. VecStore (Sorted Vector)
//! - **Algorithm**: Sorted `Vec<(Uuid, u8)>` with binary search
//! - **Performance**: ~51ns per lookup (O(log n))
//! - **Memory**: ~17 bytes per UUID (most efficient)
//! - **Best for**: Memory-constrained environments, deterministic iteration order
//! - **Advantage**: Minimal memory overhead, predictable performance
//!
//! ### 4. FullHashStore
//! - **Algorithm**: Array of 256 HashSets (one per visibility level)
//! - **Performance**: ~11-71ns depending on level (O(1) with early exit)
//! - **Memory**: Highest overhead (256 HashSets)
//! - **Best for**: Optimizing worst-case scenarios (e.g., mask=0 queries)
//! - **Advantage**: Early exit optimization when mask is low
//!
//! ## Performance Summary (2M UUIDs)
//!
//! | Implementation | Uniform | Level 0 | Higher Levels | Batch (100) |
//! |----------------|---------|---------|---------------|-------------|
//! | **HashMapStore** | **13ns** | **13ns** | **13ns** | **1.33µs** |
//! | HybridAuthStore | 67ns | 12ns | 58ns | 1.80µs |
//! | VecStore | 51ns | 51ns | 51ns | 8.04µs |
//! | FullHashStore | 66ns | 12ns | 71ns | 1.92µs |
//!
//! ## Thread Safety
//!
//! All store implementations are immutable after construction and implement `Send + Sync`,
//! making them safe to share across threads (e.g., wrapped in `Arc`).
//!
//! ## Example
//!
//! ```ignore
//! use occlusion::{load_default_from_csv, Store};
//!
//! // Load from CSV file (using recommended default implementation)
//! let store = load_default_from_csv("data.csv")?;
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
//!
//! ## Choosing an Implementation
//!
//! - **Default choice**: `HashMapStore` - fastest overall, simple, predictable O(1)
//! - **Skewed workload (90% at level 0)**: `HybridAuthStore` - comparable speed, slightly lower memory
//! - **Memory constrained**: `VecStore` - smallest memory footprint (~17 bytes/UUID)
//! - **Worst-case optimization**: `FullHashStore` - best for mask=0 queries

mod algorithm;
mod builder;
mod error;
mod loader;
mod store_fullhash;
mod store_hashmap;
mod store_hybrid;
mod store_vecstore;

pub use algorithm::StoreAlgorithm;
pub use builder::StoreBuilder;
pub use error::{Result, StoreError};
pub use loader::{
    load_default_from_csv, load_from_csv, load_fullhash_from_csv, load_hashmap_from_csv,
    load_hybrid_from_csv,
};
pub use store_fullhash::FullHashStore;
pub use store_hashmap::HashMapStore;
pub use store_hybrid::{HybridAuthStore, DistributionStats};
pub use store_vecstore::VecStore;

use std::collections::HashMap;
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

