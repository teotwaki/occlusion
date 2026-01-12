//! Error types for the occlusion store.

use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur when building or using a store.
#[derive(Error, Debug)]
pub enum StoreError {
    /// A duplicate UUID was found when building the store.
    #[error("Duplicate UUID found: {0}")]
    DuplicateUuid(Uuid),

    /// The input format was invalid.
    #[error("Invalid format: {0}")]
    InvalidFormat(String),
}

/// A specialized Result type for store operations.
pub type Result<T> = std::result::Result<T, StoreError>;
