//! Thread-safe store wrapper that supports runtime reloading.

use crate::{ActiveStore, HashMap, Store};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Thread-safe store wrapper that supports runtime reloading.
///
/// Wraps an `ActiveStore` in `Arc<RwLock<>>` to allow atomic swapping
/// of the underlying store without stopping the server.
///
/// # Performance
///
/// - Read operations acquire a read lock (very cheap when uncontended)
/// - Swap operations acquire a write lock (blocks reads briefly)
/// - No dynamic dispatch overhead (uses concrete `ActiveStore` type)
///
/// # Example
///
/// ```ignore
/// use occlusion::{SwappableStore, load_from_csv};
///
/// // Create initial store
/// let store = load_from_csv("data.csv")?;
/// let swappable = SwappableStore::new(store);
///
/// // Use in request handlers
/// let is_visible = swappable.is_visible(&uuid, 10);
///
/// // Reload with new data
/// let new_store = load_from_csv("new_data.csv")?;
/// swappable.swap(new_store);
/// ```
#[derive(Clone)]
pub struct SwappableStore {
    inner: Arc<RwLock<ActiveStore>>,
}

impl SwappableStore {
    /// Create a new SwappableStore wrapping the given store.
    pub fn new(store: ActiveStore) -> Self {
        Self {
            inner: Arc::new(RwLock::new(store)),
        }
    }

    /// Atomically swap the underlying store with a new one.
    ///
    /// This acquires a write lock, briefly blocking all read operations.
    /// The old store is dropped after the swap completes.
    pub fn swap(&self, new_store: ActiveStore) {
        let mut guard = self.inner.write().expect("RwLock poisoned");
        *guard = new_store;
    }

    /// Get the visibility level for a given UUID.
    ///
    /// Returns `None` if the UUID is not found in the store.
    #[inline]
    pub fn get_visibility(&self, uuid: &Uuid) -> Option<u8> {
        let guard = self.inner.read().expect("RwLock poisoned");
        guard.get_visibility(uuid)
    }

    /// Check if a UUID is visible under the given visibility mask.
    ///
    /// A UUID with visibility level L is visible to a request with mask M
    /// if and only if L <= M.
    #[inline]
    pub fn is_visible(&self, uuid: &Uuid, mask: u8) -> bool {
        let guard = self.inner.read().expect("RwLock poisoned");
        guard.is_visible(uuid, mask)
    }

    /// Check multiple UUIDs against the same visibility mask.
    ///
    /// Returns a vector of booleans indicating visibility for each UUID.
    pub fn check_batch(&self, uuids: &[Uuid], mask: u8) -> Vec<bool> {
        let guard = self.inner.read().expect("RwLock poisoned");
        guard.check_batch(uuids, mask)
    }

    /// Returns the total number of UUIDs in the store.
    #[inline]
    pub fn len(&self) -> usize {
        let guard = self.inner.read().expect("RwLock poisoned");
        guard.len()
    }

    /// Returns true if the store contains no UUIDs.
    #[inline]
    pub fn is_empty(&self) -> bool {
        let guard = self.inner.read().expect("RwLock poisoned");
        guard.is_empty()
    }

    /// Calculate visibility distribution statistics.
    ///
    /// Returns a map from visibility level to count of UUIDs at that level.
    pub fn visibility_distribution(&self) -> HashMap<u8, usize> {
        let guard = self.inner.read().expect("RwLock poisoned");
        guard.visibility_distribution()
    }
}

// SwappableStore is thread-safe by design
unsafe impl Send for SwappableStore {}
unsafe impl Sync for SwappableStore {}

#[cfg(test)]
mod tests {
    use super::*;

    // Import the appropriate store constructor based on active features
    #[cfg(feature = "fullhash")]
    use crate::FullHashStore as TestStore;

    #[cfg(all(feature = "hybrid", not(feature = "fullhash")))]
    use crate::HybridAuthStore as TestStore;

    #[cfg(all(feature = "vec", not(feature = "hybrid"), not(feature = "fullhash")))]
    use crate::VecStore as TestStore;

    #[cfg(not(any(feature = "vec", feature = "hybrid", feature = "fullhash")))]
    use crate::HashMapStore as TestStore;

    fn create_test_store() -> ActiveStore {
        let entries = vec![
            (Uuid::from_u128(1), 0),
            (Uuid::from_u128(2), 5),
            (Uuid::from_u128(3), 10),
        ];
        TestStore::new(entries).unwrap()
    }

    fn create_store_from_entries(entries: Vec<(Uuid, u8)>) -> ActiveStore {
        TestStore::new(entries).unwrap()
    }

    #[test]
    fn test_basic_operations() {
        let store = SwappableStore::new(create_test_store());

        assert_eq!(store.len(), 3);
        assert!(!store.is_empty());
        assert_eq!(store.get_visibility(&Uuid::from_u128(1)), Some(0));
        assert_eq!(store.get_visibility(&Uuid::from_u128(2)), Some(5));
        assert!(store.is_visible(&Uuid::from_u128(1), 0));
        assert!(!store.is_visible(&Uuid::from_u128(3), 5)); // level 10 > mask 5
    }

    #[test]
    fn test_swap() {
        let store = SwappableStore::new(create_test_store());
        assert_eq!(store.len(), 3);

        // Create new store with different data
        let new_entries = vec![(Uuid::from_u128(100), 0), (Uuid::from_u128(200), 0)];
        let new_store = create_store_from_entries(new_entries);

        store.swap(new_store);

        assert_eq!(store.len(), 2);
        assert_eq!(store.get_visibility(&Uuid::from_u128(1)), None); // Old UUID gone
        assert_eq!(store.get_visibility(&Uuid::from_u128(100)), Some(0)); // New UUID present
    }

    #[test]
    fn test_check_batch() {
        let store = SwappableStore::new(create_test_store());

        let uuids = vec![Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)];
        let results = store.check_batch(&uuids, 5);

        assert_eq!(results, vec![true, true, false]); // 0<=5, 5<=5, 10>5
    }

    #[test]
    fn test_clone() {
        let store1 = SwappableStore::new(create_test_store());
        let store2 = store1.clone();

        // Both should see the same data
        assert_eq!(store1.len(), store2.len());

        // Swap on one should affect the other (shared Arc)
        let new_entries = vec![(Uuid::from_u128(999), 0)];
        let new_store = create_store_from_entries(new_entries);
        store1.swap(new_store);

        assert_eq!(store2.len(), 1);
        assert_eq!(store2.get_visibility(&Uuid::from_u128(999)), Some(0));
    }
}
