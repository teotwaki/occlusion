//! Integration tests for the occlusion server.
//!
//! These tests verify end-to-end behavior with real data files.

use rocket::http::{ContentType, Status};
use rocket::local::blocking::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Write;
use tempfile::NamedTempFile;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
    uuid_count: usize,
}

#[derive(Debug, Deserialize)]
struct CheckResponse {
    #[allow(dead_code)]
    object: Uuid,
    is_visible: bool,
}

#[derive(Debug, Deserialize)]
struct BatchCheckResponse {
    all_visible: bool,
}

#[derive(Debug, Deserialize)]
struct StatsResponse {
    total_uuids: usize,
    #[allow(dead_code)]
    visibility_distribution: HashMap<u8, usize>,
}

#[derive(Debug, Deserialize)]
struct OpaResponse<T> {
    result: T,
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

    let source = server::source::DataSource::parse(csv_path);

    let rt = tokio::runtime::Runtime::new().unwrap();
    let (store, _metadata) = rt
        .block_on(server::loader::load(&source, None))
        .expect("Failed to load store")
        .expect("Initial load should return data");

    let swappable = SwappableStore::new(store);

    rocket::build().manage(swappable).mount(
        "/",
        rocket::routes![
            server::routes::check,
            server::routes::check_batch,
            server::routes::health,
            server::routes::stats,
            server::routes::opa_visible,
            server::routes::opa_visible_batch,
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

    // Test batch check endpoint - not all visible at mask 5
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
    assert!(!body.all_visible); // uuid3 (level 10) > mask 5

    // Test batch check endpoint - all visible at mask 10
    let response = client
        .post("/api/v1/check/batch")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"objects": ["{}", "{}", "{}"], "visibility_mask": 10}}"#,
            uuid1, uuid2, uuid3
        ))
        .dispatch();
    assert_eq!(response.status(), Status::Ok);
    let body: BatchCheckResponse = response.into_json().unwrap();
    assert!(body.all_visible);

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

    // Test OPA batch endpoint - not all visible (uuid2 level 15 > mask 10)
    let response = client
        .post("/v1/data/occlusion/visible_batch")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"input": {{"objects": ["{}", "{}"], "visibility_mask": 10}}}}"#,
            uuid1, uuid2
        ))
        .dispatch();
    assert_eq!(response.status(), Status::Ok);
    let body: OpaResponse<bool> = response.into_json().unwrap();
    assert!(!body.result);

    // Test OPA batch endpoint - all visible at mask 15
    let response = client
        .post("/v1/data/occlusion/visible_batch")
        .header(ContentType::JSON)
        .body(format!(
            r#"{{"input": {{"objects": ["{}", "{}"], "visibility_mask": 15}}}}"#,
            uuid1, uuid2
        ))
        .dispatch();
    assert_eq!(response.status(), Status::Ok);
    let body: OpaResponse<bool> = response.into_json().unwrap();
    assert!(body.result);
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
