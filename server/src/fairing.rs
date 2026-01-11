//! Request timing fairing for logging response times.

use rocket::{
    Data, Request, Response,
    fairing::{Fairing, Info, Kind},
    http::Status,
};
use std::time::Instant;
use tracing::info;

/// Fairing that logs request timing information.
pub struct RequestTimer;

/// Request-local state to store the start time.
struct TimerStart(Instant);

#[rocket::async_trait]
impl Fairing for RequestTimer {
    fn info(&self) -> Info {
        Info {
            name: "Request Timer",
            kind: Kind::Request | Kind::Response,
        }
    }

    async fn on_request(&self, request: &mut Request<'_>, _: &mut Data<'_>) {
        request.local_cache(|| TimerStart(Instant::now()));
    }

    async fn on_response<'r>(&self, request: &'r Request<'_>, response: &mut Response<'r>) {
        let start = request.local_cache(|| TimerStart(Instant::now()));
        let elapsed = start.0.elapsed();

        let method = request.method();
        let uri = request.uri().path();
        let status = response.status();

        if status == Status::NotFound && uri.as_str() == "/" {
            // Skip logging for root path 404s (common noise)
            return;
        }

        info!(
            method = %method,
            path = %uri,
            status = status.code,
            elapsed_us = elapsed.as_micros() as u64,
            "request completed"
        );
    }
}
