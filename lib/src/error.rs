use std::io;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("CSV parse error: {0}")]
    CsvError(#[from] csv::Error),

    #[error("UUID parse error: {0}")]
    UuidParse(#[from] uuid::Error),

    #[error("Invalid visibility level {level} for UUID {uuid}: must be 0-255")]
    InvalidVisibility { uuid: Uuid, level: u16 },

    #[error("Duplicate UUID found: {0}")]
    DuplicateUuid(Uuid),

    #[error("Invalid CSV format: {0}")]
    InvalidFormat(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;
