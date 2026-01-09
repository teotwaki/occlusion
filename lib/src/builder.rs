use crate::algorithm::StoreAlgorithm;
use crate::error::Result;
use crate::loader::{load_from_csv, load_fullhash_from_csv, load_hashmap_from_csv, load_hybrid_from_csv};
use crate::Store;
use std::path::Path;
use std::sync::Arc;

/// Builder for constructing store instances.
///
/// # Example
///
/// ```ignore
/// use occlusion::{StoreBuilder, StoreAlgorithm};
///
/// // Use default algorithm (HashMap)
/// let store = StoreBuilder::new()
///     .load_from_csv("data.csv")?;
///
/// // Specify algorithm
/// let store = StoreBuilder::new()
///     .algorithm(StoreAlgorithm::Hybrid)
///     .load_from_csv("data.csv")?;
/// ```
#[derive(Debug, Clone)]
pub struct StoreBuilder {
    algorithm: StoreAlgorithm,
}

impl StoreBuilder {
    /// Create a new StoreBuilder with the default algorithm (HashMap).
    pub fn new() -> Self {
        Self {
            algorithm: StoreAlgorithm::default(),
        }
    }

    /// Set the store algorithm to use.
    pub fn algorithm(mut self, algorithm: StoreAlgorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// Load a store from a CSV file using the configured algorithm.
    ///
    /// Returns a trait object that can be used polymorphically.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be read
    /// - The CSV format is invalid
    /// - Any UUID cannot be parsed
    /// - Duplicate UUIDs are found
    pub fn load_from_csv<P: AsRef<Path>>(self, path: P) -> Result<Arc<dyn Store>> {
        match self.algorithm {
            StoreAlgorithm::HashMap => {
                let store = load_hashmap_from_csv(path)?;
                Ok(Arc::new(store))
            }
            StoreAlgorithm::Vec => {
                let store = load_from_csv(path)?;
                Ok(Arc::new(store))
            }
            StoreAlgorithm::Hybrid => {
                let store = load_hybrid_from_csv(path)?;
                Ok(Arc::new(store))
            }
            StoreAlgorithm::FullHash => {
                let store = load_fullhash_from_csv(path)?;
                Ok(Arc::new(store))
            }
        }
    }

    /// Get the currently configured algorithm.
    pub fn get_algorithm(&self) -> StoreAlgorithm {
        self.algorithm
    }
}

impl Default for StoreBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_default() {
        let builder = StoreBuilder::new();
        assert_eq!(builder.get_algorithm(), StoreAlgorithm::HashMap);
    }

    #[test]
    fn test_builder_algorithm() {
        let builder = StoreBuilder::new().algorithm(StoreAlgorithm::Vec);
        assert_eq!(builder.get_algorithm(), StoreAlgorithm::Vec);
    }

    #[test]
    fn test_builder_chaining() {
        let builder = StoreBuilder::new()
            .algorithm(StoreAlgorithm::Hybrid)
            .algorithm(StoreAlgorithm::Vec);
        assert_eq!(builder.get_algorithm(), StoreAlgorithm::Vec);
    }
}
