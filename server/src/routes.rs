use crate::models::*;
use occlusion::Store;
use rocket::serde::json::Json;
use rocket::State;
use std::sync::Arc;

/// Check if a single object is visible under the given visibility mask
///
/// POST /api/v1/check
#[post("/api/v1/check", data = "<request>")]
pub fn check(
    store: &State<Arc<dyn Store>>,
    request: Json<CheckRequest>,
) -> Json<CheckResponse> {
    let is_visible = store.is_visible(&request.object, request.visibility_mask);

    Json(CheckResponse {
        object: request.object,
        is_visible,
    })
}

/// Check multiple objects against the same visibility mask
///
/// POST /api/v1/check/batch
#[post("/api/v1/check/batch", data = "<request>")]
pub fn check_batch(
    store: &State<Arc<dyn Store>>,
    request: Json<BatchCheckRequest>,
) -> Json<BatchCheckResponse> {
    let results = request
        .objects
        .iter()
        .map(|object| {
            let is_visible = store.is_visible(object, request.visibility_mask);
            CheckResponse {
                object: *object,
                is_visible,
            }
        })
        .collect();

    Json(BatchCheckResponse { results })
}

/// Health check endpoint
///
/// GET /health
#[get("/health")]
pub fn health(store: &State<Arc<dyn Store>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        uuid_count: store.len(),
    })
}

/// Get statistics about the store
///
/// GET /api/v1/stats
#[get("/api/v1/stats")]
pub fn stats(store: &State<Arc<dyn Store>>) -> Json<StatsResponse> {
    Json(StatsResponse {
        total_uuids: store.len(),
        visibility_distribution: store.visibility_distribution(),
    })
}
