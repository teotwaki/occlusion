use crate::{HashMap, HashSet, Store};
use uuid::Uuid;

/// Full hash-based authorization store using one HashSet per visibility level.
///
/// Uses 256 HashSets (one per visibility level 0-255) for O(1) lookups with early exit.
/// This trades memory for optimized worst-case performance (e.g., mask=0 queries).
///
/// ## When to Use
/// - Need to optimize worst-case scenarios (mask=0 queries: ~11ns)
/// - Want guaranteed O(1) lookups regardless of distribution
/// - Memory is not a constraint
///
/// ## Performance (2M UUIDs, with FxHash)
/// - Level 0 lookup: ~2.3ns
/// - Higher level lookup: ~21ns (checks multiple levels with early exit)
/// - Worst case (mask=0): ~6.2ns (best of all implementations)
/// - Batch (100): ~422ns
/// - Memory: Highest overhead (256 HashSets)
#[derive(Debug, Clone)]
pub struct FullHashStore {
    /// One HashSet per visibility level (0-255)
    by_level: [HashSet<Uuid>; 256],
}

impl FullHashStore {
    /// Create a new FullHashStore from a vector of (UUID, visibility) pairs.
    ///
    /// Each UUID is placed in the HashSet corresponding to its visibility level.
    /// Duplicates will cause an error to be returned.
    pub fn new(entries: Vec<(Uuid, u8)>) -> Result<Self, String> {
        // Initialize 256 empty HashSets
        let mut by_level: [HashSet<Uuid>; 256] = std::array::from_fn(|_| Default::default());

        let mut all_uuids: HashSet<_> = Default::default();

        // Insert each UUID into its corresponding level's HashSet
        for (uuid, level) in entries {
            // Check for duplicates across all levels
            if !all_uuids.insert(uuid) {
                return Err(format!("Duplicate UUID found: {}", uuid));
            }
            by_level[level as usize].insert(uuid);
        }

        Ok(Self { by_level })
    }

    /// Returns statistics about the store distribution.
    pub fn distribution_stats(&self) -> DistributionStats {
        let total = self.len();
        let level_0_count = self.by_level[0].len();

        DistributionStats {
            total_uuids: total,
            level_0_count,
            higher_levels_count: total - level_0_count,
            level_0_percentage: if total > 0 {
                (level_0_count as f64 / total as f64) * 100.0
            } else {
                0.0
            },
        }
    }
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

// FullHashStore is immutable after construction, so it's safe to share across threads
unsafe impl Send for FullHashStore {}
unsafe impl Sync for FullHashStore {}

impl crate::Store for FullHashStore {
    #[inline]
    fn get_visibility(&self, uuid: &Uuid) -> Option<u8> {
        // Search through levels 0-255 until we find the UUID
        for (level, set) in self.by_level.iter().enumerate() {
            if set.contains(uuid) {
                return Some(level as u8);
            }
        }
        None
    }

    #[inline]
    fn is_visible(&self, uuid: &Uuid, mask: u8) -> bool {
        // Only check levels 0 through mask (inclusive)
        // Early exit as soon as we find the UUID
        for level in 0..=mask {
            if self.by_level[level as usize].contains(uuid) {
                return true;
            }
        }
        false
    }

    fn check_batch(&self, uuids: &[Uuid], mask: u8) -> Vec<bool> {
        uuids
            .iter()
            .map(|uuid| self.is_visible(uuid, mask))
            .collect()
    }

    #[inline]
    fn len(&self) -> usize {
        self.by_level.iter().map(|set| set.len()).sum()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.by_level.iter().all(|set| set.is_empty())
    }

    fn visibility_distribution(&self) -> HashMap<u8, usize> {
        let mut dist: HashMap<_, _> = Default::default();

        for (level, set) in self.by_level.iter().enumerate() {
            if !set.is_empty() {
                dist.insert(level as u8, set.len());
            }
        }

        dist
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uuid_from_u128(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn test_new_partitions_correctly() {
        let entries = vec![
            (uuid_from_u128(1), 0),
            (uuid_from_u128(2), 5),
            (uuid_from_u128(3), 0),
            (uuid_from_u128(4), 10),
        ];
        let store = FullHashStore::new(entries).unwrap();

        assert_eq!(store.by_level[0].len(), 2);
        assert_eq!(store.by_level[5].len(), 1);
        assert_eq!(store.by_level[10].len(), 1);
    }

    #[test]
    fn test_duplicate_detection() {
        let uuid = uuid_from_u128(42);

        // Duplicate in same level
        let entries = vec![(uuid, 0), (uuid, 0)];
        assert!(FullHashStore::new(entries).is_err());

        // Duplicate in different levels (still an error)
        let entries = vec![(uuid, 0), (uuid, 5)];
        assert!(FullHashStore::new(entries).is_err());
    }

    #[test]
    fn test_get_visibility() {
        let uuid0 = uuid_from_u128(1);
        let uuid5 = uuid_from_u128(2);
        let uuid10 = uuid_from_u128(3);
        let uuid_missing = uuid_from_u128(999);

        let entries = vec![(uuid0, 0), (uuid5, 5), (uuid10, 10)];
        let store = FullHashStore::new(entries).unwrap();

        assert_eq!(store.get_visibility(&uuid0), Some(0));
        assert_eq!(store.get_visibility(&uuid5), Some(5));
        assert_eq!(store.get_visibility(&uuid10), Some(10));
        assert_eq!(store.get_visibility(&uuid_missing), None);
    }

    #[test]
    fn test_is_visible_level_0() {
        let uuid = uuid_from_u128(1);
        let entries = vec![(uuid, 0)];
        let store = FullHashStore::new(entries).unwrap();

        // Level 0 is visible at all masks
        assert_eq!(store.is_visible(&uuid, 0), true);
        assert_eq!(store.is_visible(&uuid, 10), true);
        assert_eq!(store.is_visible(&uuid, 255), true);
    }

    #[test]
    fn test_is_visible_higher_levels() {
        let uuid = uuid_from_u128(1);
        let entries = vec![(uuid, 8)];
        let store = FullHashStore::new(entries).unwrap();

        assert_eq!(store.is_visible(&uuid, 10), true); // 8 <= 10
        assert_eq!(store.is_visible(&uuid, 8), true); // 8 <= 8
        assert_eq!(store.is_visible(&uuid, 7), false); // 8 > 7
        assert_eq!(store.is_visible(&uuid, 0), false); // 8 > 0
    }

    #[test]
    fn test_is_visible_missing_uuid() {
        let uuid = uuid_from_u128(999);
        let entries = vec![(uuid_from_u128(1), 0)];
        let store = FullHashStore::new(entries).unwrap();

        assert_eq!(store.is_visible(&uuid, 255), false);
    }

    #[test]
    fn test_check_batch() {
        let uuid1 = uuid_from_u128(1);
        let uuid2 = uuid_from_u128(2);
        let uuid3 = uuid_from_u128(3);

        let entries = vec![(uuid1, 0), (uuid2, 10), (uuid3, 15)];
        let store = FullHashStore::new(entries).unwrap();

        let results = store.check_batch(&[uuid1, uuid2, uuid3], 10);
        assert_eq!(results, vec![true, true, false]);
    }

    #[test]
    fn test_len_and_is_empty() {
        let empty_store = FullHashStore::new(vec![]).unwrap();
        assert!(empty_store.is_empty());
        assert_eq!(empty_store.len(), 0);

        let store = FullHashStore::new(vec![(uuid_from_u128(1), 5)]).unwrap();
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_distribution_stats() {
        let entries = vec![
            (uuid_from_u128(1), 0),
            (uuid_from_u128(2), 0),
            (uuid_from_u128(3), 0),
            (uuid_from_u128(4), 5),
        ];
        let store = FullHashStore::new(entries).unwrap();

        let stats = store.distribution_stats();
        assert_eq!(stats.total_uuids, 4);
        assert_eq!(stats.level_0_count, 3);
        assert_eq!(stats.higher_levels_count, 1);
        assert!((stats.level_0_percentage - 75.0).abs() < 0.01);
    }
}
