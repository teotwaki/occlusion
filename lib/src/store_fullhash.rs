use crate::{HashMap, HashSet, StoreError};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Full hash-based authorization store using one HashSet per visibility level.
///
/// Uses a sparse BTreeMap of HashSets, only allocating for levels that have entries.
/// Provides O(1) lookups per level with early exit on `is_visible`.
///
/// ## When to Use
/// - Need to optimize worst-case scenarios (mask=0 queries)
/// - Want guaranteed O(1) lookups regardless of distribution
/// - Data uses a small subset of the 256 possible visibility levels
///
/// ## Performance (2M UUIDs, with FxHash)
/// - Level 0 lookup: ~2.3ns
/// - Higher level lookup: ~21ns (checks multiple levels with early exit)
/// - Worst case (mask=0): ~6.2ns (best of all implementations)
/// - Batch (100): ~422ns
#[derive(Debug, Clone)]
pub struct FullHashStore {
    /// Sparse map of visibility level -> UUIDs at that level
    by_level: BTreeMap<u8, HashSet<Uuid>>,
    /// Total count of UUIDs
    total: usize,
}

impl FullHashStore {
    /// Create a new FullHashStore from a vector of (UUID, visibility) pairs.
    ///
    /// Each UUID is placed in the HashSet corresponding to its visibility level.
    /// Only levels with entries are allocated.
    /// Duplicates will cause an error to be returned.
    pub fn new(entries: Vec<(Uuid, u8)>) -> Result<Self, StoreError> {
        let mut by_level: BTreeMap<u8, HashSet<Uuid>> = BTreeMap::new();
        let mut all_uuids: HashSet<Uuid> = Default::default();

        for (uuid, level) in entries {
            if !all_uuids.insert(uuid) {
                return Err(StoreError::DuplicateUuid(uuid));
            }
            by_level.entry(level).or_default().insert(uuid);
        }

        let total = all_uuids.len();
        Ok(Self { by_level, total })
    }

    /// Returns statistics about the store distribution.
    pub fn distribution_stats(&self) -> DistributionStats {
        let level_0_count = self.by_level.get(&0).map_or(0, |s| s.len());

        DistributionStats {
            total_uuids: self.total,
            level_0_count,
            higher_levels_count: self.total - level_0_count,
            level_0_percentage: if self.total > 0 {
                (level_0_count as f64 / self.total as f64) * 100.0
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

impl crate::Store for FullHashStore {
    #[inline]
    fn is_visible(&self, uuid: &Uuid, mask: u8) -> bool {
        self.by_level
            .range(..=mask)
            .any(|(_, set)| set.contains(uuid))
    }

    fn check_batch(&self, uuids: &[Uuid], mask: u8) -> bool {
        uuids.iter().all(|uuid| self.is_visible(uuid, mask))
    }

    #[inline]
    fn len(&self) -> usize {
        self.total
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.total == 0
    }

    fn visibility_distribution(&self) -> HashMap<u8, usize> {
        self.by_level
            .iter()
            .map(|(&level, set)| (level, set.len()))
            .collect()
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
    fn test_new_partitions_correctly() {
        let entries = vec![
            (uuid_from_u128(1), 0),
            (uuid_from_u128(2), 5),
            (uuid_from_u128(3), 0),
            (uuid_from_u128(4), 10),
        ];
        let store = FullHashStore::new(entries).unwrap();

        assert_eq!(store.by_level.get(&0).unwrap().len(), 2);
        assert_eq!(store.by_level.get(&5).unwrap().len(), 1);
        assert_eq!(store.by_level.get(&10).unwrap().len(), 1);
        assert_eq!(store.by_level.len(), 3); // Only 3 levels allocated
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

        // All visible at mask 15
        assert!(store.check_batch(&[uuid1, uuid2, uuid3], 15));
        // Not all visible at mask 10 (uuid3 has level 15)
        assert!(!store.check_batch(&[uuid1, uuid2, uuid3], 10));
        // Subset that is all visible
        assert!(store.check_batch(&[uuid1, uuid2], 10));
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
