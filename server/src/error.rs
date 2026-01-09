use occlusion::StoreError;
use thiserror::Error;

/// Server-specific errors.
#[derive(Error, Debug)]
pub enum ServerError {
    /// Error loading the store from CSV
    #[error("Failed to load store: {0}")]
    StoreLoad(#[from] StoreError),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Type alias for server Results
pub type Result<T> = std::result::Result<T, ServerError>;
