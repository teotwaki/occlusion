use crate::HashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Sorted vector authorization store containing UUID-visibility mappings.
///
/// UUIDs are stored in a sorted vector for O(log n) binary search lookups.
/// This provides predictable performance (~51ns for millions of entries)
/// with minimal memory overhead (~17 bytes per entry).
///
/// ## When to Use
/// - Memory-constrained environments (smallest memory footprint)
/// - When you need deterministic iteration order
/// - Predictable, consistent performance is more important than raw speed
///
/// ## Performance (2M UUIDs)
/// - Point lookup: ~51ns
/// - Batch (100): ~8µs
/// - Memory: ~17 bytes/UUID (most efficient)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VecStore {
    /// Sorted array of (UUID, visibility_level) pairs
    entries: Vec<(Uuid, u8)>,
}

impl VecStore {
    /// Create a new VecStore from a vector of (UUID, visibility) pairs.
    ///
    /// The entries will be sorted by UUID for efficient binary search.
    /// Duplicates will cause an error to be returned.
    pub fn new(mut entries: Vec<(Uuid, u8)>) -> Result<Self, String> {
        // Sort by UUID
        entries.sort_unstable_by_key(|(uuid, _)| *uuid);

        // Check for duplicates
        for window in entries.windows(2) {
            if window[0].0 == window[1].0 {
                return Err(format!("Duplicate UUID found: {}", window[0].0));
            }
        }

        Ok(Self { entries })
    }
}

// VecStore is immutable after construction, so it's safe to share across threads
unsafe impl Send for VecStore {}
unsafe impl Sync for VecStore {}

impl crate::Store for VecStore {
    fn get_visibility(&self, uuid: &uuid::Uuid) -> Option<u8> {
        self.entries
            .binary_search_by_key(uuid, |(u, _)| *u)
            .ok()
            .map(|idx| self.entries[idx].1)
    }

    #[inline]
    fn is_visible(&self, uuid: &uuid::Uuid, mask: u8) -> bool {
        self.get_visibility(uuid)
            .map(|level| level <= mask)
            .unwrap_or(false)
    }

    fn check_batch(&self, uuids: &[uuid::Uuid], mask: u8) -> Vec<bool> {
        uuids
            .iter()
            .map(|uuid| self.is_visible(uuid, mask))
            .collect()
    }

    #[inline]
    fn len(&self) -> usize {
        self.entries.len()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn visibility_distribution(&self) -> HashMap<u8, usize> {
        let mut dist: HashMap<_, _> = Default::default();

        for (_, level) in &self.entries {
            *dist.entry(*level).or_insert(0) += 1;
        }

        dist
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;
    use uuid::Uuid;

    fn uuid_from_u128(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn test_new_sorts_entries() {
        let entries = vec![
            (uuid_from_u128(3), 10),
            (uuid_from_u128(1), 5),
            (uuid_from_u128(2), 15),
        ];
        let store = VecStore::new(entries).unwrap();
        assert_eq!(store.entries[0].0, uuid_from_u128(1));
        assert_eq!(store.entries[1].0, uuid_from_u128(2));
        assert_eq!(store.entries[2].0, uuid_from_u128(3));
    }

    #[test]
    fn test_duplicate_detection() {
        let uuid = uuid_from_u128(42);
        let entries = vec![(uuid, 10), (uuid, 20)];
        assert!(VecStore::new(entries).is_err());
    }

    #[test]
    fn test_get_visibility() {
        let uuid1 = uuid_from_u128(1);
        let uuid2 = uuid_from_u128(2);
        let uuid3 = uuid_from_u128(3);

        let entries = vec![(uuid1, 5), (uuid2, 10)];
        let store = VecStore::new(entries).unwrap();

        assert_eq!(store.get_visibility(&uuid1), Some(5));
        assert_eq!(store.get_visibility(&uuid2), Some(10));
        assert_eq!(store.get_visibility(&uuid3), None);
    }

    #[test]
    fn test_is_visible() {
        let uuid = uuid_from_u128(1);
        let entries = vec![(uuid, 8)];
        let store = VecStore::new(entries).unwrap();

        assert!(store.is_visible(&uuid, 10)); // 8 <= 10
        assert!(store.is_visible(&uuid, 8)); // 8 <= 8
        assert!(!store.is_visible(&uuid, 7)); // 8 > 7

        let unknown_uuid = uuid_from_u128(999);
        assert!(!store.is_visible(&unknown_uuid, 255));
    }

    #[test]
    fn test_check_batch() {
        let uuid1 = uuid_from_u128(1);
        let uuid2 = uuid_from_u128(2);
        let uuid3 = uuid_from_u128(3);

        let entries = vec![(uuid1, 5), (uuid2, 10), (uuid3, 15)];
        let store = VecStore::new(entries).unwrap();

        let results = store.check_batch(&[uuid1, uuid2, uuid3], 10);
        assert_eq!(results, vec![true, true, false]);
    }

    #[test]
    fn test_len_and_is_empty() {
        let empty_store = VecStore::new(vec![]).unwrap();
        assert!(empty_store.is_empty());
        assert_eq!(empty_store.len(), 0);

        let store = VecStore::new(vec![(uuid_from_u128(1), 5)]).unwrap();
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_visibility_distribution() {
        let entries = vec![
            (uuid_from_u128(1), 5),
            (uuid_from_u128(2), 5),
            (uuid_from_u128(3), 10),
            (uuid_from_u128(4), 5),
        ];
        let store = VecStore::new(entries).unwrap();

        let dist = store.visibility_distribution();
        assert_eq!(dist.get(&5), Some(&3));
        assert_eq!(dist.get(&10), Some(&1));
        assert_eq!(dist.get(&15), None);
    }
}
