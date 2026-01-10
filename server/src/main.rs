#[macro_use]
extern crate rocket;

mod error;
mod loader;
mod models;
mod routes;
mod source;

use clap::Parser;
use error::Result;
use loader::{load_from_source, reload_if_changed};
use occlusion::{Store, SwappableStore};
use rocket::figment::Figment;
use source::{DataSource, SourceMetadata};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// High-performance authorization server for UUID visibility lookups
#[derive(Parser, Debug)]
#[command(name = "occlusion")]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to CSV file or URL (http:// or https://)
    #[arg(value_name = "DATA_SOURCE", env = "OCCLUSION_DATA_SOURCE")]
    data_source: String,

    /// Reload interval in minutes (0 = no auto-reload)
    #[arg(long, default_value = "0", env = "OCCLUSION_RELOAD_INTERVAL")]
    reload_interval: u64,

    /// Output logs as JSON
    #[arg(long, env = "OCCLUSION_JSON_LOGS")]
    json_logs: bool,
}

/// Shared state for the reload scheduler
pub struct ReloadState {
    pub source: DataSource,
    pub metadata: RwLock<SourceMetadata>,
}

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
async fn load_store_async(source: &DataSource) -> Result<(SwappableStore, SourceMetadata)> {
    info!(source = %source, "Loading authorization store");

    let (store, metadata) = load_from_source(source).await?;

    info!(uuid_count = store.len(), "Store loaded successfully");

    Ok((SwappableStore::new(store), metadata))
}

/// Spawn the reload scheduler task
fn spawn_reload_scheduler(
    store: SwappableStore,
    reload_state: Arc<ReloadState>,
    interval_mins: u64,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_mins * 60));

        // Skip the first tick (fires immediately)
        interval.tick().await;

        loop {
            interval.tick().await;

            info!(source = %reload_state.source, "Checking for data source changes");

            let old_metadata = {
                let metadata = reload_state.metadata.read().expect("RwLock poisoned");
                metadata.clone()
            };

            match reload_if_changed(&reload_state.source, &old_metadata).await {
                Ok(Some((new_store, new_metadata))) => {
                    let count = new_store.len();
                    store.swap(new_store);

                    let mut metadata = reload_state.metadata.write().expect("RwLock poisoned");
                    *metadata = new_metadata;

                    info!(uuid_count = count, "Store reloaded successfully");
                }
                Ok(None) => {
                    info!("Source unchanged, skipping reload");
                }
                Err(e) => {
                    error!(error = %e, "Failed to reload store, keeping existing data");
                }
            }
        }
    });
}

#[launch]
async fn rocket() -> _ {
    let args = Args::parse();
    init_tracing(args.json_logs);

    let source = DataSource::parse(&args.data_source);

    let (store, metadata) = match load_store_async(&source).await {
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
        spawn_reload_scheduler(store.clone(), reload_state.clone(), args.reload_interval);
    }

    info!("Starting Rocket server");

    let figment = Figment::from(rocket::Config::default()).merge(("cli_colors", false));

    rocket::custom(figment)
        .manage(store)
        .manage(reload_state)
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
                routes::opa_level,
                // Admin API
                routes::reload,
            ],
        )
}
