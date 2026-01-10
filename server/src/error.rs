use occlusion::StoreError;
use thiserror::Error;

/// Errors that can occur during data loading.
#[derive(Error, Debug)]
pub enum LoadError {
    /// IO error (file not found, permission denied, etc.)
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// CSV parsing error
    #[error("CSV parse error: {0}")]
    CsvError(#[from] csv::Error),

    /// HTTP request error
    #[error("HTTP error: {0}")]
    HttpError(String),

    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    ReqwestError(#[from] reqwest::Error),

    /// Invalid data format
    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    /// Store construction error
    #[error("Store error: {0}")]
    StoreError(#[from] StoreError),
}

/// Type alias for loading Results
pub type Result<T> = std::result::Result<T, LoadError>;
