use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(not(feature = "nofx"))]
use rustc_hash::FxHashMap as HashMap;

#[cfg(feature = "nofx")]
use std::collections::HashMap;

/// Request to check if a single object is visible
#[derive(Debug, Deserialize, Serialize)]
pub struct CheckRequest {
    pub object: Uuid,
    pub visibility_mask: u8,
}

/// Response for a single object visibility check
#[derive(Debug, Deserialize, Serialize)]
pub struct CheckResponse {
    pub object: Uuid,
    pub is_visible: bool,
}

/// Request to check multiple objects at once
#[derive(Debug, Deserialize, Serialize)]
pub struct BatchCheckRequest {
    pub objects: Vec<Uuid>,
    pub visibility_mask: u8,
}

/// Response for batch object visibility check
#[derive(Debug, Deserialize, Serialize)]
pub struct BatchCheckResponse {
    pub results: Vec<CheckResponse>,
}

/// Health check response
#[derive(Debug, Deserialize, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub uuid_count: usize,
}

/// Statistics response
#[derive(Debug, Deserialize, Serialize)]
pub struct StatsResponse {
    pub total_uuids: usize,
    pub visibility_distribution: HashMap<u8, usize>,
}
