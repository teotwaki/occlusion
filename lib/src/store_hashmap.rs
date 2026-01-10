use crate::HashMap;
use uuid::Uuid;

/// Pure HashMap-based authorization store (RECOMMENDED DEFAULT).
///
/// Uses a single `HashMap<Uuid, u8>` for pure O(1) lookups.
/// This is the simplest and fastest implementation in almost all scenarios.
///
/// ## When to Use
/// - **Default choice for most applications**
/// - Unknown or uniform distribution
/// - Need consistent, predictable O(1) performance
/// - Simplicity and speed are priorities
///
/// ## Performance (2M UUIDs, with FxHash)
/// - All lookups: ~2.7ns (consistent O(1))
/// - Batch (100): ~347ns (fastest)
/// - Memory: ~24-32 bytes/UUID (standard HashMap overhead)
///
/// ## Advantages
/// - **Fastest** in uniform distributions (4x faster than sorted)
/// - **Simplest** implementation (single HashMap)
/// - **Consistent** performance regardless of visibility level or mask
/// - Competitive with specialized implementations even on skewed workloads
#[derive(Debug, Clone)]
pub struct HashMapStore {
    /// HashMap mapping UUID to visibility level
    map: HashMap<Uuid, u8>,
}

impl HashMapStore {
    /// Create a new HashMapStore from a vector of (UUID, visibility) pairs.
    ///
    /// Duplicates will cause an error to be returned.
    pub fn new(entries: Vec<(Uuid, u8)>) -> Result<Self, String> {
        #[cfg(not(feature = "nofx"))]
        let mut map = HashMap::with_capacity_and_hasher(entries.len(), Default::default());

        #[cfg(feature = "nofx")]
        let mut map = HashMap::with_capacity(entries.len());

        for (uuid, level) in entries {
            if map.insert(uuid, level).is_some() {
                return Err(format!("Duplicate UUID found: {}", uuid));
            }
        }

        Ok(Self { map })
    }
}

// HashMapStore is immutable after construction, so it's safe to share across threads
unsafe impl Send for HashMapStore {}
unsafe impl Sync for HashMapStore {}

impl crate::Store for HashMapStore {
    #[inline]
    fn get_visibility(&self, uuid: &Uuid) -> Option<u8> {
        self.map.get(uuid).copied()
    }

    fn is_visible(&self, uuid: &Uuid, mask: u8) -> bool {
        self.map
            .get(uuid)
            .map(|level| *level <= mask)
            .unwrap_or(false)
    }

    fn check_batch(&self, uuids: &[Uuid], mask: u8) -> Vec<bool> {
        uuids
            .iter()
            .map(|uuid| self.is_visible(uuid, mask))
            .collect()
    }

    #[inline]
    fn len(&self) -> usize {
        self.map.len()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    fn visibility_distribution(&self) -> HashMap<u8, usize> {
        let mut dist = HashMap::default();

        for level in self.map.values() {
            *dist.entry(*level).or_insert(0) += 1;
        }

        dist
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;

    fn uuid_from_u128(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn test_new_stores_correctly() {
        let entries = vec![
            (uuid_from_u128(1), 0),
            (uuid_from_u128(2), 5),
            (uuid_from_u128(3), 10),
        ];
        let store = HashMapStore::new(entries).unwrap();

        assert_eq!(store.map.len(), 3);
    }

    #[test]
    fn test_duplicate_detection() {
        let uuid = uuid_from_u128(42);
        let entries = vec![(uuid, 0), (uuid, 5)];
        assert!(HashMapStore::new(entries).is_err());
    }

    #[test]
    fn test_get_visibility() {
        let uuid0 = uuid_from_u128(1);
        let uuid5 = uuid_from_u128(2);
        let uuid10 = uuid_from_u128(3);
        let uuid_missing = uuid_from_u128(999);

        let entries = vec![(uuid0, 0), (uuid5, 5), (uuid10, 10)];
        let store = HashMapStore::new(entries).unwrap();

        assert_eq!(Store::get_visibility(&store, &uuid0), Some(0));
        assert_eq!(Store::get_visibility(&store, &uuid5), Some(5));
        assert_eq!(Store::get_visibility(&store, &uuid10), Some(10));
        assert_eq!(Store::get_visibility(&store, &uuid_missing), None);
    }

    #[test]
    fn test_is_visible_level_0() {
        let uuid = uuid_from_u128(1);
        let entries = vec![(uuid, 0)];
        let store = HashMapStore::new(entries).unwrap();

        // Level 0 is visible at all masks
        assert!(Store::is_visible(&store, &uuid, 0));
        assert!(Store::is_visible(&store, &uuid, 10));
        assert!(Store::is_visible(&store, &uuid, 255));
    }

    #[test]
    fn test_is_visible_higher_levels() {
        let uuid = uuid_from_u128(1);
        let entries = vec![(uuid, 8)];
        let store = HashMapStore::new(entries).unwrap();

        assert!(Store::is_visible(&store, &uuid, 10)); // 8 <= 10
        assert!(Store::is_visible(&store, &uuid, 8)); // 8 <= 8
        assert!(!Store::is_visible(&store, &uuid, 7)); // 8 > 7
        assert!(!Store::is_visible(&store, &uuid, 0)); // 8 > 0
    }

    #[test]
    fn test_is_visible_missing_uuid() {
        let uuid = uuid_from_u128(999);
        let entries = vec![(uuid_from_u128(1), 0)];
        let store = HashMapStore::new(entries).unwrap();

        assert!(!Store::is_visible(&store, &uuid, 255));
    }

    #[test]
    fn test_check_batch() {
        let uuid1 = uuid_from_u128(1);
        let uuid2 = uuid_from_u128(2);
        let uuid3 = uuid_from_u128(3);

        let entries = vec![(uuid1, 0), (uuid2, 10), (uuid3, 15)];
        let store = HashMapStore::new(entries).unwrap();

        let results = Store::check_batch(&store, &[uuid1, uuid2, uuid3], 10);
        assert_eq!(results, vec![true, true, false]);
    }

    #[test]
    fn test_len_and_is_empty() {
        let empty_store = HashMapStore::new(vec![]).unwrap();
        assert!(Store::is_empty(&empty_store));
        assert_eq!(Store::len(&empty_store), 0);

        let store = HashMapStore::new(vec![(uuid_from_u128(1), 5)]).unwrap();
        assert!(!Store::is_empty(&store));
        assert_eq!(Store::len(&store), 1);
    }
}
