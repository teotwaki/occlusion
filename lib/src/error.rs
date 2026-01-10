use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("Duplicate UUID found: {0}")]
    DuplicateUuid(Uuid),

    #[error("Invalid format: {0}")]
    InvalidFormat(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;
