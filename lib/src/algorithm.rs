use std::fmt;
use std::str::FromStr;

/// Store algorithm implementation variants.
///
/// Determines which data structure implementation to use for storing and querying UUIDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreAlgorithm {
    /// HashMap-based store (recommended default)
    ///
    /// - Performance: ~13ns per lookup (O(1))
    /// - Memory: ~24-32 bytes per UUID
    /// - Best for: All workloads, especially uniform distributions
    HashMap,

    /// Sorted vector with binary search
    ///
    /// - Performance: ~51ns per lookup (O(log n))
    /// - Memory: ~17 bytes per UUID (most efficient)
    /// - Best for: Memory-constrained environments
    Vec,

    /// Hybrid: HashSet for level 0 + sorted vector for higher levels
    ///
    /// - Performance: ~12ns for level 0, ~58ns for higher levels
    /// - Memory: ~24 bytes per UUID
    /// - Best for: Skewed distributions (80-90% at level 0)
    Hybrid,

    /// Array of 256 HashSets (one per visibility level)
    ///
    /// - Performance: ~11-71ns depending on level (O(1) with early exit)
    /// - Memory: Highest overhead
    /// - Best for: Optimizing worst-case scenarios (mask=0 queries)
    FullHash,
}

impl StoreAlgorithm {
    /// Returns the default algorithm (HashMap).
    pub const fn default() -> Self {
        Self::HashMap
    }

    /// Returns all available algorithms.
    pub const fn all() -> &'static [Self] {
        &[Self::HashMap, Self::Vec, Self::Hybrid, Self::FullHash]
    }

    /// Returns the algorithm name as a string.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::HashMap => "hashmap",
            Self::Vec => "vec",
            Self::Hybrid => "hybrid",
            Self::FullHash => "fullhash",
        }
    }
}

impl fmt::Display for StoreAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for StoreAlgorithm {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "hashmap" => Ok(Self::HashMap),
            "vec" => Ok(Self::Vec),
            "hybrid" => Ok(Self::Hybrid),
            "fullhash" => Ok(Self::FullHash),
            _ => Err(format!(
                "Invalid algorithm '{}'. Valid options: hashmap, vec, hybrid, fullhash",
                s
            )),
        }
    }
}

impl Default for StoreAlgorithm {
    fn default() -> Self {
        Self::HashMap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str() {
        assert_eq!("hashmap".parse::<StoreAlgorithm>().unwrap(), StoreAlgorithm::HashMap);
        assert_eq!("vec".parse::<StoreAlgorithm>().unwrap(), StoreAlgorithm::Vec);
        assert_eq!("hybrid".parse::<StoreAlgorithm>().unwrap(), StoreAlgorithm::Hybrid);
        assert_eq!("fullhash".parse::<StoreAlgorithm>().unwrap(), StoreAlgorithm::FullHash);

        // Case insensitive
        assert_eq!("HASHMAP".parse::<StoreAlgorithm>().unwrap(), StoreAlgorithm::HashMap);

        // Invalid
        assert!("invalid".parse::<StoreAlgorithm>().is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(StoreAlgorithm::HashMap.to_string(), "hashmap");
        assert_eq!(StoreAlgorithm::Vec.to_string(), "vec");
        assert_eq!(StoreAlgorithm::Hybrid.to_string(), "hybrid");
        assert_eq!(StoreAlgorithm::FullHash.to_string(), "fullhash");
    }

    #[test]
    fn test_default() {
        assert_eq!(StoreAlgorithm::default(), StoreAlgorithm::HashMap);
    }
}
