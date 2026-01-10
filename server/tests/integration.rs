//! Integration tests for the occlusion server.
//!
//! These tests verify end-to-end behavior with real data files.

use rocket::http::{ContentType, Status};
use rocket::local::blocking::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use tempfile::NamedTempFile;
use uuid::Uuid;

// Import types we need for deserialization
#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
    uuid_count: usize,
}

#[derive(Debug, Deserialize)]
struct CheckResponse {
    object: Uuid,
    is_visible: bool,
}

#[derive(Debug, Deserialize)]
struct BatchCheckResponse {
    results: Vec<CheckResponse>,
}

#[derive(Debug, Deserialize)]
struct StatsResponse {
    total_uuids: usize,
    visibility_distribution: HashMap<u8, usize>,
}

#[derive(Debug, Deserialize)]
struct OpaResponse<T> {
    result: T,
}

#[derive(Debug, Deserialize)]
struct ReloadResponse {
    success: bool,
    uuid_count: usize,
    message: String,
}

/// Create a test CSV file with the given entries.
fn create_test_csv(entries: &[(Uuid, u8)]) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("Failed to create temp file");
    writeln!(file, "uuid,visibility_level").expect("Failed to write header");
    for (uuid, level) in entries {
        writeln!(file, "{},{}", uuid, level).expect("Failed to write entry");
    }
    file.flush().expect("Failed to flush file");
    file
}

/// Build a test rocket instance from a CSV file path.
fn build_test_rocket(csv_path: &str) -> rocket::Rocket<rocket::Build> {
    use occlusion::SwappableStore;
    use std::sync::{Arc, RwLock};

    // Parse the data source
    let source = server::source::DataSource::parse(csv_path);

    // Load the store synchronously for testing
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (store, metadata) = rt
        .block_on(server::loader::load_from_source(&source))
        .expect("Failed to load store");

    let swappable = SwappableStore::new(store);
    let reload_state = Arc::new(server::ReloadState {
        source: source.clone(),
        metadata: RwLock::new(metadata),
    });

    rocket::build()
        .manage(swappable)
        .manage(reload_state)
        .mount(
            "/",
            rocket::routes![
                server::routes::check,
                server::routes::check_batch,
                server::routes::health,
                server::routes::stats,
                server::routes::opa_visible,
                server::routes::opa_visible_batch,
                server::routes::opa_level,
                server::routes::reload,
            ],
        )
}

#[test]
fn test_server_with_csv_file() {
    let uuid1 = Uuid::from_u128(1);
    let uuid2 = Uuid::from_u128(2);
    let uuid3 = Uuid::from_u128(3);

    let entries = vec![(uuid1, 0), (uuid2, 5), (uuid3, 10)];
    let csv_file = create_test_csv(&entries);

    let rocket = build_test_rocket(csv_file.path().to_str().unwrap());
    let client = Client::tracked(rocket).expect("valid rocket instance");

    // Test health endpoint
    let response = client.get("/health").dispatch();
    assert_eq!(response.status(), Status::Ok);
    let body: HealthResponse = response.into_json().unwrap();
    assert_eq!(body.status, "ok");
    assert_eq!(body.uuid_count, 3);

    // Test check endpoint
    let response = client
        .post("/api/v1/check")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"object": "{}", "visibility_mask": 5}}"#,
            uuid2
        ))
        .dispatch();
    assert_eq!(response.status(), Status::Ok);
    let body: CheckResponse = response.into_json().unwrap();
    assert!(body.is_visible); // Level 5 <= mask 5

    // Test batch check endpoint
    let response = client
        .post("/api/v1/check/batch")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"objects": ["{}", "{}", "{}"], "visibility_mask": 5}}"#,
            uuid1, uuid2, uuid3
        ))
        .dispatch();
    assert_eq!(response.status(), Status::Ok);
    let body: BatchCheckResponse = response.into_json().unwrap();
    assert_eq!(body.results.len(), 3);
    assert!(body.results[0].is_visible); // Level 0 <= 5
    assert!(body.results[1].is_visible); // Level 5 <= 5
    assert!(!body.results[2].is_visible); // Level 10 > 5

    // Test stats endpoint
    let response = client.get("/api/v1/stats").dispatch();
    assert_eq!(response.status(), Status::Ok);
    let body: StatsResponse = response.into_json().unwrap();
    assert_eq!(body.total_uuids, 3);
}

#[test]
fn test_opa_endpoints_with_csv_file() {
    let uuid1 = Uuid::from_u128(100);
    let uuid2 = Uuid::from_u128(200);

    let entries = vec![(uuid1, 0), (uuid2, 15)];
    let csv_file = create_test_csv(&entries);

    let rocket = build_test_rocket(csv_file.path().to_str().unwrap());
    let client = Client::tracked(rocket).expect("valid rocket instance");

    // Test OPA visible endpoint
    let response = client
        .post("/v1/data/occlusion/visible")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"input": {{"object": "{}", "visibility_mask": 10}}}}"#,
            uuid1
        ))
        .dispatch();
    assert_eq!(response.status(), Status::Ok);
    let body: OpaResponse<bool> = response.into_json().unwrap();
    assert!(body.result);

    // Test OPA level endpoint
    let response = client
        .post("/v1/data/occlusion/level")
        .header(ContentType::JSON)
        .body(format!(r#"{{"input": {{"object": "{}"}}}}"#, uuid2))
        .dispatch();
    assert_eq!(response.status(), Status::Ok);
    let body: OpaResponse<Option<u8>> = response.into_json().unwrap();
    assert_eq!(body.result, Some(15));

    // Test OPA batch endpoint
    let response = client
        .post("/v1/data/occlusion/visible_batch")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"input": {{"objects": ["{}", "{}"], "visibility_mask": 10}}}}"#,
            uuid1, uuid2
        ))
        .dispatch();
    assert_eq!(response.status(), Status::Ok);
    let body: OpaResponse<HashMap<Uuid, bool>> = response.into_json().unwrap();
    assert_eq!(body.result.get(&uuid1), Some(&true));
    assert_eq!(body.result.get(&uuid2), Some(&false));
}

#[test]
fn test_reload_endpoint() {
    let uuid1 = Uuid::from_u128(1);
    let entries = vec![(uuid1, 5)];

    // Create initial CSV file
    let csv_file = create_test_csv(&entries);
    let csv_path = csv_file.path().to_str().unwrap().to_string();

    let rocket = build_test_rocket(&csv_path);
    let client = Client::tracked(rocket).expect("valid rocket instance");

    // Verify initial state
    let response = client.get("/health").dispatch();
    let body: HealthResponse = response.into_json().unwrap();
    assert_eq!(body.uuid_count, 1);

    // Update the CSV file with new data
    let uuid2 = Uuid::from_u128(2);
    let uuid3 = Uuid::from_u128(3);
    fs::write(
        &csv_path,
        format!("uuid,visibility_level\n{},0\n{},5\n{},10\n", uuid1, uuid2, uuid3),
    )
    .expect("Failed to update CSV");

    // Trigger reload
    let response = client.post("/api/v1/admin/reload").dispatch();
    assert_eq!(response.status(), Status::Ok);
    let body: ReloadResponse = response.into_json().unwrap();
    assert!(body.success);
    assert_eq!(body.uuid_count, 3);

    // Verify new state
    let response = client.get("/health").dispatch();
    let body: HealthResponse = response.into_json().unwrap();
    assert_eq!(body.uuid_count, 3);
}

#[test]
fn test_empty_csv() {
    let entries: Vec<(Uuid, u8)> = vec![];
    let csv_file = create_test_csv(&entries);

    let rocket = build_test_rocket(csv_file.path().to_str().unwrap());
    let client = Client::tracked(rocket).expect("valid rocket instance");

    let response = client.get("/health").dispatch();
    assert_eq!(response.status(), Status::Ok);
    let body: HealthResponse = response.into_json().unwrap();
    assert_eq!(body.uuid_count, 0);

    // Query a non-existent UUID should return false
    let response = client
        .post("/api/v1/check")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"object": "{}", "visibility_mask": 255}}"#,
            Uuid::from_u128(1)
        ))
        .dispatch();
    assert_eq!(response.status(), Status::Ok);
    let body: CheckResponse = response.into_json().unwrap();
    assert!(!body.is_visible);
}

#[test]
fn test_visibility_boundaries() {
    let uuid = Uuid::from_u128(42);
    let entries = vec![(uuid, 128)]; // Mid-range visibility level
    let csv_file = create_test_csv(&entries);

    let rocket = build_test_rocket(csv_file.path().to_str().unwrap());
    let client = Client::tracked(rocket).expect("valid rocket instance");

    // Test at exact boundary
    let response = client
        .post("/api/v1/check")
        .header(ContentType::JSON)
        .body(format!(r#"{{"object": "{}", "visibility_mask": 128}}"#, uuid))
        .dispatch();
    let body: CheckResponse = response.into_json().unwrap();
    assert!(body.is_visible); // 128 <= 128

    // Test just below
    let response = client
        .post("/api/v1/check")
        .header(ContentType::JSON)
        .body(format!(r#"{{"object": "{}", "visibility_mask": 127}}"#, uuid))
        .dispatch();
    let body: CheckResponse = response.into_json().unwrap();
    assert!(!body.is_visible); // 128 > 127

    // Test at maximum
    let response = client
        .post("/api/v1/check")
        .header(ContentType::JSON)
        .body(format!(r#"{{"object": "{}", "visibility_mask": 255}}"#, uuid))
        .dispatch();
    let body: CheckResponse = response.into_json().unwrap();
    assert!(body.is_visible); // 128 <= 255
}
