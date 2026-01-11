use crate::models::*;
use occlusion::{Store, SwappableStore};
use rocket::State;
use rocket::serde::json::Json;

/// Check if a single object is visible under the given visibility mask
///
/// POST /api/v1/check
#[post("/api/v1/check", data = "<request>")]
pub fn check(store: &State<SwappableStore>, request: Json<CheckRequest>) -> Json<CheckResponse> {
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
    store: &State<SwappableStore>,
    request: Json<BatchCheckRequest>,
) -> Json<BatchCheckResponse> {
    let all_visible = store.check_batch(&request.objects, request.visibility_mask);
    Json(BatchCheckResponse { all_visible })
}

/// Health check endpoint
///
/// GET /health
#[get("/health")]
pub fn health(store: &State<SwappableStore>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        uuid_count: store.len(),
    })
}

/// Get statistics about the store
///
/// GET /api/v1/stats
#[get("/api/v1/stats")]
pub fn stats(store: &State<SwappableStore>) -> Json<StatsResponse> {
    Json(StatsResponse {
        total_uuids: store.len(),
        visibility_distribution: store.visibility_distribution().into_iter().collect(),
    })
}

// ============================================================================
// OPA-Compatible Endpoints
// ============================================================================

/// OPA-compatible visibility check
///
/// POST /v1/data/occlusion/visible
///
/// Request: `{"input": {"object": "uuid", "visibility_mask": 10}}`
/// Response: `{"result": true}`
#[post("/v1/data/occlusion/visible", data = "<request>")]
pub fn opa_visible(
    store: &State<SwappableStore>,
    request: Json<OpaRequest<OpaVisibleInput>>,
) -> Json<OpaResponse<bool>> {
    let is_visible = store.is_visible(&request.input.object, request.input.visibility_mask);
    Json(OpaResponse { result: is_visible })
}

/// OPA-compatible batch visibility check
///
/// POST /v1/data/occlusion/visible_batch
///
/// Request: `{"input": {"objects": ["uuid1", "uuid2"], "visibility_mask": 10}}`
/// Response: `{"result": true}` (true if all objects are visible)
#[post("/v1/data/occlusion/visible_batch", data = "<request>")]
pub fn opa_visible_batch(
    store: &State<SwappableStore>,
    request: Json<OpaRequest<OpaBatchVisibleInput>>,
) -> Json<OpaResponse<bool>> {
    let all_visible = store.check_batch(&request.input.objects, request.input.visibility_mask);
    Json(OpaResponse { result: all_visible })
}


#[cfg(test)]
mod tests {
    use super::*;
    use rocket::http::{ContentType, Status};
    use rocket::local::blocking::Client;
    use uuid::Uuid;

    // Import the appropriate store constructor based on active features
    #[cfg(feature = "fullhash")]
    use occlusion::FullHashStore as TestStore;

    #[cfg(all(feature = "hybrid", not(feature = "fullhash")))]
    use occlusion::HybridAuthStore as TestStore;

    #[cfg(all(feature = "vec", not(feature = "hybrid"), not(feature = "fullhash")))]
    use occlusion::VecStore as TestStore;

    #[cfg(not(any(feature = "vec", feature = "hybrid", feature = "fullhash")))]
    use occlusion::HashMapStore as TestStore;

    fn create_test_client() -> Client {
        // Create a store with test data
        let entries = vec![
            (Uuid::from_u128(1), 0),  // Level 0 - visible to all
            (Uuid::from_u128(2), 5),  // Level 5
            (Uuid::from_u128(3), 10), // Level 10
            (Uuid::from_u128(4), 15), // Level 15
        ];
        let store = TestStore::new(entries).unwrap();
        let swappable = SwappableStore::new(store);

        let rocket = rocket::build().manage(swappable).mount(
            "/",
            routes![
                check,
                check_batch,
                health,
                stats,
                opa_visible,
                opa_visible_batch,
            ],
        );

        Client::tracked(rocket).expect("valid rocket instance")
    }

    fn uuid_str(n: u128) -> String {
        Uuid::from_u128(n).to_string()
    }

    // ========================================================================
    // Original API Tests
    // ========================================================================

    #[test]
    fn test_health() {
        let client = create_test_client();
        let response = client.get("/health").dispatch();

        assert_eq!(response.status(), Status::Ok);
        let body: HealthResponse = response.into_json().unwrap();
        assert_eq!(body.status, "ok");
        assert_eq!(body.uuid_count, 4);
    }

    #[test]
    fn test_check_visible() {
        let client = create_test_client();
        let response = client
            .post("/api/v1/check")
            .header(ContentType::JSON)
            .body(format!(
                r#"{{"object": "{}", "visibility_mask": 10}}"#,
                uuid_str(2)
            ))
            .dispatch();

        assert_eq!(response.status(), Status::Ok);
        let body: CheckResponse = response.into_json().unwrap();
        assert!(body.is_visible); // Level 5 <= mask 10
    }

    #[test]
    fn test_check_not_visible() {
        let client = create_test_client();
        let response = client
            .post("/api/v1/check")
            .header(ContentType::JSON)
            .body(format!(
                r#"{{"object": "{}", "visibility_mask": 10}}"#,
                uuid_str(4)
            ))
            .dispatch();

        assert_eq!(response.status(), Status::Ok);
        let body: CheckResponse = response.into_json().unwrap();
        assert!(!body.is_visible); // Level 15 > mask 10
    }

    #[test]
    fn test_check_batch() {
        let client = create_test_client();

        // Not all visible at mask 10 (uuid4 has level 15)
        let response = client
            .post("/api/v1/check/batch")
            .header(ContentType::JSON)
            .body(format!(
                r#"{{"objects": ["{}", "{}", "{}"], "visibility_mask": 10}}"#,
                uuid_str(1),
                uuid_str(2),
                uuid_str(4)
            ))
            .dispatch();
        assert_eq!(response.status(), Status::Ok);
        let body: BatchCheckResponse = response.into_json().unwrap();
        assert!(!body.all_visible);

        // All visible at mask 15
        let response = client
            .post("/api/v1/check/batch")
            .header(ContentType::JSON)
            .body(format!(
                r#"{{"objects": ["{}", "{}", "{}"], "visibility_mask": 15}}"#,
                uuid_str(1),
                uuid_str(2),
                uuid_str(4)
            ))
            .dispatch();
        assert_eq!(response.status(), Status::Ok);
        let body: BatchCheckResponse = response.into_json().unwrap();
        assert!(body.all_visible);
    }

    #[test]
    fn test_stats() {
        let client = create_test_client();
        let response = client.get("/api/v1/stats").dispatch();

        assert_eq!(response.status(), Status::Ok);
        let body: StatsResponse = response.into_json().unwrap();
        assert_eq!(body.total_uuids, 4);
        assert_eq!(body.visibility_distribution.get(&0), Some(&1));
        assert_eq!(body.visibility_distribution.get(&5), Some(&1));
        assert_eq!(body.visibility_distribution.get(&10), Some(&1));
        assert_eq!(body.visibility_distribution.get(&15), Some(&1));
    }

    // ========================================================================
    // OPA-Compatible API Tests
    // ========================================================================

    #[test]
    fn test_opa_visible_true() {
        let client = create_test_client();
        let response = client
            .post("/v1/data/occlusion/visible")
            .header(ContentType::JSON)
            .body(format!(
                r#"{{"input": {{"object": "{}", "visibility_mask": 10}}}}"#,
                uuid_str(2)
            ))
            .dispatch();

        assert_eq!(response.status(), Status::Ok);
        let body: OpaResponse<bool> = response.into_json().unwrap();
        assert!(body.result); // Level 5 <= mask 10
    }

    #[test]
    fn test_opa_visible_false() {
        let client = create_test_client();
        let response = client
            .post("/v1/data/occlusion/visible")
            .header(ContentType::JSON)
            .body(format!(
                r#"{{"input": {{"object": "{}", "visibility_mask": 10}}}}"#,
                uuid_str(4)
            ))
            .dispatch();

        assert_eq!(response.status(), Status::Ok);
        let body: OpaResponse<bool> = response.into_json().unwrap();
        assert!(!body.result); // Level 15 > mask 10
    }

    #[test]
    fn test_opa_visible_not_found() {
        let client = create_test_client();
        let response = client
            .post("/v1/data/occlusion/visible")
            .header(ContentType::JSON)
            .body(format!(
                r#"{{"input": {{"object": "{}", "visibility_mask": 255}}}}"#,
                uuid_str(999) // Non-existent UUID
            ))
            .dispatch();

        assert_eq!(response.status(), Status::Ok);
        let body: OpaResponse<bool> = response.into_json().unwrap();
        assert!(!body.result); // Not found = not visible
    }

    #[test]
    fn test_opa_visible_batch() {
        let client = create_test_client();

        // Not all visible at mask 10 (uuid4 has level 15)
        let response = client
            .post("/v1/data/occlusion/visible_batch")
            .header(ContentType::JSON)
            .body(format!(
                r#"{{"input": {{"objects": ["{}", "{}", "{}"], "visibility_mask": 10}}}}"#,
                uuid_str(1),
                uuid_str(2),
                uuid_str(4)
            ))
            .dispatch();
        assert_eq!(response.status(), Status::Ok);
        let body: OpaResponse<bool> = response.into_json().unwrap();
        assert!(!body.result);

        // All visible at mask 15
        let response = client
            .post("/v1/data/occlusion/visible_batch")
            .header(ContentType::JSON)
            .body(format!(
                r#"{{"input": {{"objects": ["{}", "{}", "{}"], "visibility_mask": 15}}}}"#,
                uuid_str(1),
                uuid_str(2),
                uuid_str(4)
            ))
            .dispatch();
        assert_eq!(response.status(), Status::Ok);
        let body: OpaResponse<bool> = response.into_json().unwrap();
        assert!(body.result);
    }

}
