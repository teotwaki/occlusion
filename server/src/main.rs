#[macro_use]
extern crate rocket;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use clap::{Parser, ValueEnum};
use occlusion::{Store, SwappableStore};
use rocket::figment::Figment;
use server::{
    ReloadState,
    error::Result,
    fairing::RequestTimer,
    loader::load,
    routes,
    source::{DataSource, SourceMetadata},
};
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Action to take when max reload failures is exceeded.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum FailureAction {
    /// Shut down the server
    #[default]
    Shutdown,
    /// Clear the store (replace with empty data)
    Clear,
}

/// High-performance authorization server for UUID visibility lookups
#[derive(Parser, Debug)]
#[command(name = "occlusion")]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to CSV file or URL (http:// or https://)
    #[cfg(not(feature = "static-url"))]
    #[arg(value_name = "DATA_SOURCE", env = "OCCLUSION_DATA_SOURCE")]
    data_source: String,

    /// Reload interval in minutes (0 = no auto-reload)
    #[arg(long, default_value = "60", env = "OCCLUSION_RELOAD_INTERVAL")]
    reload_interval: u64,

    /// Maximum consecutive reload failures before taking action (0 = unlimited)
    #[arg(long, default_value = "0", env = "OCCLUSION_MAX_RELOAD_FAILURES")]
    max_reload_failures: u32,

    /// Action to take when max reload failures is exceeded
    #[arg(long, default_value = "shutdown", env = "OCCLUSION_ON_MAX_FAILURES")]
    on_max_failures: FailureAction,

    /// Output logs as JSON
    #[arg(long, env = "OCCLUSION_JSON_LOGS")]
    json_logs: bool,
}

#[cfg(all(feature = "static-url", debug_assertions))]
const STATIC_DATA_SOURCE: &str = match option_env!("OCCLUSION_STATIC_URL") {
    Some(url) => url,
    None => "https://example.com/data.csv",
};

#[cfg(all(feature = "static-url", not(debug_assertions)))]
const STATIC_DATA_SOURCE: &str = env!("OCCLUSION_STATIC_URL");

/// Initialize tracing subscriber for structured logging
fn init_tracing(json: bool) {
    let env_filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());

    if json {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }
}

/// Load the store from the data source (async for URL support)
async fn load_store(source: &DataSource) -> Result<(SwappableStore, SourceMetadata)> {
    info!(source = %source, "Loading authorization store");

    let (store, metadata) = load(source, None)
        .await?
        .expect("Initial load should always return data");

    info!(uuid_count = store.len(), "Store loaded successfully");

    Ok((SwappableStore::new(store), metadata))
}

/// Initial backoff delay on failure (5 seconds).
const INITIAL_BACKOFF_SECS: u64 = 5;
/// Maximum backoff delay (5 minutes).
const MAX_BACKOFF_SECS: u64 = 300;

/// Result of recording a failure in the tracker.
enum FailureResponse {
    /// Retry after the given backoff duration.
    Backoff(Duration),
    /// Max failures exceeded, take the configured action.
    MaxExceeded(FailureAction),
}

/// Tracks consecutive reload failures with exponential backoff.
struct FailureTracker {
    consecutive: u32,
    max: u32,
    action: FailureAction,
}

impl FailureTracker {
    fn new(max: u32, action: FailureAction) -> Self {
        Self {
            consecutive: 0,
            max,
            action,
        }
    }

    fn reset(&mut self) {
        self.consecutive = 0;
    }

    fn record(&mut self) -> FailureResponse {
        self.consecutive = self.consecutive.saturating_add(1);

        if self.max > 0 && self.consecutive >= self.max {
            FailureResponse::MaxExceeded(self.action)
        } else {
            // Exponential backoff: 5s, 10s, 20s, 40s, ... capped at MAX_BACKOFF_SECS
            let backoff_secs =
                (INITIAL_BACKOFF_SECS << (self.consecutive - 1)).min(MAX_BACKOFF_SECS);
            FailureResponse::Backoff(Duration::from_secs(backoff_secs))
        }
    }

    fn count(&self) -> u32 {
        self.consecutive
    }
}

/// Spawn the reload scheduler task with exponential backoff on failures.
fn spawn_reload_scheduler(
    store: SwappableStore,
    reload_state: Arc<ReloadState>,
    interval_mins: u64,
    max_failures: u32,
    on_max_failures: FailureAction,
) {
    tokio::spawn(async move {
        let base_interval = Duration::from_secs(interval_mins * 60);
        let mut failures = FailureTracker::new(max_failures, on_max_failures);

        // Initial delay before first check
        tokio::time::sleep(base_interval).await;

        loop {
            info!(source = %reload_state.source, "Checking for data source changes");

            let old_metadata = {
                let guard = reload_state.metadata.read().expect("RwLock poisoned");
                guard.clone()
            };

            match load(&reload_state.source, Some(&old_metadata)).await {
                Ok(Some((new_store, new_metadata))) => {
                    let count = new_store.len();
                    store.swap(new_store);

                    let mut guard = reload_state.metadata.write().expect("RwLock poisoned");
                    *guard = new_metadata;

                    failures.reset();
                    info!(uuid_count = count, "Store reloaded successfully");
                }
                Ok(None) => {
                    failures.reset();
                    info!("Source unchanged, skipping reload");
                }
                Err(e) => match failures.record() {
                    FailureResponse::Backoff(backoff) => {
                        error!(
                            error = %e,
                            consecutive_failures = failures.count(),
                            next_retry_secs = backoff.as_secs(),
                            "Failed to reload store, keeping existing data"
                        );
                        tokio::time::sleep(backoff).await;
                        continue;
                    }
                    FailureResponse::MaxExceeded(action) => {
                        error!(
                            error = %e,
                            consecutive_failures = failures.count(),
                            "Max reload failures exceeded"
                        );
                        match action {
                            FailureAction::Shutdown => {
                                error!("Shutting down due to reload failures");
                                std::process::exit(1);
                            }
                            FailureAction::Clear => {
                                error!("Clearing store due to reload failures");
                                let empty = occlusion::build_store(vec![])
                                    .expect("Failed to build empty store");
                                store.swap(empty);
                                failures.reset();
                            }
                        }
                    }
                },
            }

            tokio::time::sleep(base_interval).await;
        }
    });
}

#[launch]
async fn rocket() -> _ {
    let args = Args::parse();
    init_tracing(args.json_logs);

    #[cfg(feature = "static-url")]
    let source = DataSource::parse(STATIC_DATA_SOURCE);
    #[cfg(not(feature = "static-url"))]
    let source = DataSource::parse(&args.data_source);

    let (store, metadata) = match load_store(&source).await {
        Ok(result) => result,
        Err(e) => {
            error!(error = %e, "Failed to start server");
            std::process::exit(1);
        }
    };

    let reload_state = Arc::new(ReloadState {
        source: source.clone(),
        metadata: RwLock::new(metadata),
    });

    if args.reload_interval > 0 {
        info!(
            interval_mins = args.reload_interval,
            "Starting reload scheduler"
        );
        spawn_reload_scheduler(
            store.clone(),
            reload_state.clone(),
            args.reload_interval,
            args.max_reload_failures,
            args.on_max_failures,
        );
    }

    info!("Starting occlusion server");

    let figment = Figment::from(rocket::Config::default())
        .merge(("cli_colors", false))
        .merge(("ident", concat!("occlusion/", env!("CARGO_PKG_VERSION"))));

    rocket::custom(figment)
        .attach(RequestTimer)
        .manage(store)
        .mount(
            "/",
            routes![
                // Original API
                routes::check,
                routes::check_batch,
                routes::health,
                routes::stats,
                // OPA-compatible API
                routes::opa_visible,
                routes::opa_visible_batch,
            ],
        )
}
