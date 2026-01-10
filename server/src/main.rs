#[macro_use]
extern crate rocket;

mod error;
mod models;
mod routes;

use clap::Parser;
use error::Result;
use occlusion::{
    DataSource, SourceMetadata, Store, SwappableStore, check_source_changed, load_from_source,
};
use rocket::figment::Figment;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// High-performance authorization server for UUID visibility lookups
#[derive(Parser, Debug)]
#[command(name = "occlusion")]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to CSV file or URL (http:// or https://)
    #[arg(value_name = "DATA_SOURCE")]
    data_source: String,

    /// Reload interval in minutes (0 = no auto-reload)
    #[arg(long, default_value = "0")]
    reload_interval: u64,
}

/// Shared state for the reload scheduler
pub struct ReloadState {
    pub source: DataSource,
    pub metadata: RwLock<SourceMetadata>,
}

/// Initialize tracing subscriber for structured logging
fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Load the store from the data source
fn load_store(source: &DataSource) -> Result<(SwappableStore, SourceMetadata)> {
    info!(source = %source, "Loading authorization store");

    let (store, metadata) = load_from_source(source)?;

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

            // Check if the source has changed
            let should_reload = {
                let metadata = reload_state.metadata.read().expect("RwLock poisoned");
                match check_source_changed(&reload_state.source, &metadata) {
                    Ok(changed) => changed,
                    Err(e) => {
                        warn!(error = %e, "Failed to check source changes, will attempt reload");
                        true
                    }
                }
            };

            if !should_reload {
                info!("Source unchanged, skipping reload");
                continue;
            }

            info!("Source changed, reloading store");

            match load_from_source(&reload_state.source) {
                Ok((new_store, new_metadata)) => {
                    let count = new_store.len();
                    store.swap(new_store);

                    // Update metadata
                    let mut metadata = reload_state.metadata.write().expect("RwLock poisoned");
                    *metadata = new_metadata;

                    info!(uuid_count = count, "Store reloaded successfully");
                }
                Err(e) => {
                    error!(error = %e, "Failed to reload store, keeping existing data");
                }
            }
        }
    });
}

#[launch]
fn rocket() -> _ {
    // Initialize tracing
    init_tracing();

    // Parse command line arguments
    let args = Args::parse();

    // Parse the data source
    let source = DataSource::parse(&args.data_source);

    // Load the initial store
    let (store, metadata) = match load_store(&source) {
        Ok(result) => result,
        Err(e) => {
            error!(error = %e, "Failed to start server");
            std::process::exit(1);
        }
    };

    // Create reload state for the scheduler
    let reload_state = Arc::new(ReloadState {
        source: source.clone(),
        metadata: RwLock::new(metadata),
    });

    // Start the reload scheduler if interval > 0
    if args.reload_interval > 0 {
        info!(
            interval_mins = args.reload_interval,
            "Starting reload scheduler"
        );
        spawn_reload_scheduler(store.clone(), reload_state.clone(), args.reload_interval);
    }

    info!("Starting Rocket server");

    // Configure Rocket with colors disabled
    let figment = Figment::from(rocket::Config::default()).merge(("cli_colors", false));

    // Build and launch Rocket server
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
