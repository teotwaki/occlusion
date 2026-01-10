use crate::{HashMap, HashSet, Store};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Hybrid authorization store optimized for skewed distributions.
///
/// Uses a HashSet for visibility level 0 (fast O(1) lookup) and a sorted
/// array for higher visibility levels (O(log n) binary search).
///
/// This is optimized for workloads where 80-90% of UUIDs have visibility 0.
/// For such distributions, this provides ~4x faster average-case performance
/// compared to pure binary search.
///
/// ## When to Use
/// - Known skewed distribution (80-90% at level 0)
/// - Need similar performance to HashMap but with slightly lower memory for the hot path
/// - Want optimized early exit for mask=0 queries
///
/// ## Performance (2M UUIDs, 90% at level 0, with FxHash)
/// - Level 0 lookup: ~2.5ns (90% of queries)
/// - Higher level lookup: ~48ns (10% of queries)
/// - Batch (100): ~780ns
/// - Memory: ~24 bytes/UUID for level 0, ~17 for others
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridAuthStore {
    /// HashSet for O(1) lookup of UUIDs at visibility level 0
    level_0: HashSet<Uuid>,

    /// Sorted array for binary search of UUIDs at visibility levels 1-255
    higher_levels: Vec<(Uuid, u8)>,
}

impl HybridAuthStore {
    /// Create a new HybridAuthStore from a vector of (UUID, visibility) pairs.
    ///
    /// Entries at visibility level 0 go into a HashSet, others into a sorted array.
    /// Duplicates will cause an error to be returned.
    pub fn new(entries: Vec<(Uuid, u8)>) -> Result<Self, String> {
        let mut level_0: HashSet<_> = Default::default();
        let mut higher_levels = Vec::new();

        // Partition by visibility level
        for (uuid, level) in entries {
            if level == 0 {
                if !level_0.insert(uuid) {
                    return Err(format!("Duplicate UUID found: {}", uuid));
                }
            } else {
                higher_levels.push((uuid, level));
            }
        }

        // Sort higher levels by UUID for binary search
        higher_levels.sort_unstable_by_key(|(uuid, _)| *uuid);

        // Check for duplicates in higher levels
        for window in higher_levels.windows(2) {
            if window[0].0 == window[1].0 {
                return Err(format!("Duplicate UUID found: {}", window[0].0));
            }
        }

        // Check for UUIDs that appear in both level_0 and higher_levels
        for (uuid, _) in &higher_levels {
            if level_0.contains(uuid) {
                return Err(format!("Duplicate UUID found: {}", uuid));
            }
        }

        Ok(Self {
            level_0,
            higher_levels,
        })
    }

    /// Returns statistics about the store distribution.
    ///
    /// Useful for understanding if the hybrid approach is beneficial.
    pub fn distribution_stats(&self) -> DistributionStats {
        let total = self.len();
        let level_0_count = self.level_0.len();

        DistributionStats {
            total_uuids: total,
            level_0_count,
            higher_levels_count: self.higher_levels.len(),
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

// HybridAuthStore is immutable after construction, so it's safe to share across threads
unsafe impl Send for HybridAuthStore {}
unsafe impl Sync for HybridAuthStore {}

impl crate::Store for HybridAuthStore {
    #[inline]
    fn get_visibility(&self, uuid: &Uuid) -> Option<u8> {
        // Fast path: check level 0 first
        if self.level_0.contains(uuid) {
            return Some(0);
        }

        // Slow path: binary search higher levels
        self.higher_levels
            .binary_search_by_key(uuid, |(u, _)| *u)
            .ok()
            .map(|idx| self.higher_levels[idx].1)
    }

    #[inline]
    fn is_visible(&self, uuid: &Uuid, mask: u8) -> bool {
        // Fast path: check level 0 first (90% probability)
        // Since level 0 is always <= any mask (u8), we just check presence
        if self.level_0.contains(uuid) {
            return true;
        }

        // Early exit: if mask is 0 and not in level_0, it's not visible
        if mask == 0 {
            return false;
        }

        // Slow path: binary search higher levels and compare
        self.higher_levels
            .binary_search_by_key(uuid, |(u, _)| *u)
            .ok()
            .map(|idx| self.higher_levels[idx].1 <= mask)
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
        self.level_0.len() + self.higher_levels.len()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.level_0.is_empty() && self.higher_levels.is_empty()
    }

    fn visibility_distribution(&self) -> HashMap<u8, usize> {
        let mut dist: HashMap<_, _> = Default::default();

        if !self.level_0.is_empty() {
            dist.insert(0, self.level_0.len());
        }

        for (_, level) in &self.higher_levels {
            *dist.entry(*level).or_insert(0) += 1;
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
        let store = HybridAuthStore::new(entries).unwrap();

        assert_eq!(store.level_0.len(), 2);
        assert_eq!(store.higher_levels.len(), 2);
    }

    #[test]
    fn test_duplicate_detection() {
        let uuid = uuid_from_u128(42);

        // Duplicate in level 0
        let entries = vec![(uuid, 0), (uuid, 0)];
        assert!(HybridAuthStore::new(entries).is_err());

        // Duplicate in higher levels
        let entries = vec![(uuid, 5), (uuid, 10)];
        assert!(HybridAuthStore::new(entries).is_err());

        // Duplicate across level 0 and higher
        let entries = vec![(uuid, 0), (uuid, 5)];
        assert!(HybridAuthStore::new(entries).is_err());
    }

    #[test]
    fn test_get_visibility() {
        let uuid0 = uuid_from_u128(1);
        let uuid5 = uuid_from_u128(2);
        let uuid10 = uuid_from_u128(3);
        let uuid_missing = uuid_from_u128(999);

        let entries = vec![(uuid0, 0), (uuid5, 5), (uuid10, 10)];
        let store = HybridAuthStore::new(entries).unwrap();

        assert_eq!(store.get_visibility(&uuid0), Some(0));
        assert_eq!(store.get_visibility(&uuid5), Some(5));
        assert_eq!(store.get_visibility(&uuid10), Some(10));
        assert_eq!(store.get_visibility(&uuid_missing), None);
    }

    #[test]
    fn test_is_visible_level_0() {
        let uuid = uuid_from_u128(1);
        let entries = vec![(uuid, 0)];
        let store = HybridAuthStore::new(entries).unwrap();

        // Level 0 is visible at all masks
        assert!(store.is_visible(&uuid, 0));
        assert!(store.is_visible(&uuid, 10));
        assert!(store.is_visible(&uuid, 255));
    }

    #[test]
    fn test_is_visible_higher_levels() {
        let uuid = uuid_from_u128(1);
        let entries = vec![(uuid, 8)];
        let store = HybridAuthStore::new(entries).unwrap();

        assert!(store.is_visible(&uuid, 10)); // 8 <= 10
        assert!(store.is_visible(&uuid, 8)); // 8 <= 8
        assert!(!store.is_visible(&uuid, 7)); // 8 > 7
        assert!(!store.is_visible(&uuid, 0)); // 8 > 0
    }

    #[test]
    fn test_is_visible_missing_uuid() {
        let uuid = uuid_from_u128(999);
        let entries = vec![(uuid_from_u128(1), 0)];
        let store = HybridAuthStore::new(entries).unwrap();

        assert!(!store.is_visible(&uuid, 255));
    }

    #[test]
    fn test_check_batch() {
        let uuid1 = uuid_from_u128(1);
        let uuid2 = uuid_from_u128(2);
        let uuid3 = uuid_from_u128(3);

        let entries = vec![(uuid1, 0), (uuid2, 10), (uuid3, 15)];
        let store = HybridAuthStore::new(entries).unwrap();

        let results = store.check_batch(&[uuid1, uuid2, uuid3], 10);
        assert_eq!(results, vec![true, true, false]);
    }

    #[test]
    fn test_distribution_stats() {
        let entries = vec![
            (uuid_from_u128(1), 0),
            (uuid_from_u128(2), 0),
            (uuid_from_u128(3), 0),
            (uuid_from_u128(4), 5),
        ];
        let store = HybridAuthStore::new(entries).unwrap();

        let stats = store.distribution_stats();
        assert_eq!(stats.total_uuids, 4);
        assert_eq!(stats.level_0_count, 3);
        assert_eq!(stats.higher_levels_count, 1);
        assert!((stats.level_0_percentage - 75.0).abs() < 0.01);
    }

    #[test]
    fn test_skewed_distribution() {
        // Simulate 90% at level 0, 10% at higher levels
        let mut entries = Vec::new();
        for i in 0..900 {
            entries.push((uuid_from_u128(i), 0));
        }
        for i in 900..1000 {
            entries.push((uuid_from_u128(i), (i % 255) as u8 + 1));
        }

        let store = HybridAuthStore::new(entries).unwrap();
        let stats = store.distribution_stats();

        assert_eq!(stats.level_0_count, 900);
        assert_eq!(stats.higher_levels_count, 100);
        assert!((stats.level_0_percentage - 90.0).abs() < 0.01);
    }
}
