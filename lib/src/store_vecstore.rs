use crate::{HashMap, StoreError};
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
#[derive(Debug, Clone)]
pub struct VecStore {
    /// Sorted array of (UUID, visibility_level) pairs
    entries: Vec<(Uuid, u8)>,
}

impl VecStore {
    /// Create a new VecStore from a vector of (UUID, visibility) pairs.
    ///
    /// The entries will be sorted by UUID for efficient binary search.
    /// Duplicates will cause an error to be returned.
    pub fn new(mut entries: Vec<(Uuid, u8)>) -> Result<Self, StoreError> {
        entries.sort_unstable_by_key(|(uuid, _)| *uuid);

        if let Some(dup) = entries.windows(2).find(|w| w[0].0 == w[1].0) {
            return Err(StoreError::DuplicateUuid(dup[0].0));
        }

        entries.shrink_to_fit();
        Ok(Self { entries })
    }
}

impl crate::Store for VecStore {
    #[inline]
    fn is_visible(&self, uuid: &uuid::Uuid, mask: u8) -> bool {
        self.entries
            .binary_search_by_key(uuid, |(u, _)| *u)
            .ok()
            .map(|idx| self.entries[idx].1 <= mask)
            .unwrap_or(false)
    }

    fn check_batch(&self, uuids: &[uuid::Uuid], mask: u8) -> bool {
        uuids.iter().all(|uuid| self.is_visible(uuid, mask))
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
        self.entries
            .iter()
            .fold(HashMap::default(), |mut acc, (_, level)| {
                *acc.entry(*level).or_insert(0) += 1;
                acc
            })
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

        // All visible at mask 15
        assert!(store.check_batch(&[uuid1, uuid2, uuid3], 15));
        // Not all visible at mask 10 (uuid3 has level 15)
        assert!(!store.check_batch(&[uuid1, uuid2, uuid3], 10));
        // Subset that is all visible
        assert!(store.check_batch(&[uuid1, uuid2], 10));
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
