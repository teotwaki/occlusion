#[macro_use]
extern crate rocket;

mod error;
mod models;
mod routes;

use clap::Parser;
use error::Result;
use occlusion::{ActiveStore, Store, load_from_csv};
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// High-performance authorization server for UUID visibility lookups
#[derive(Parser, Debug)]
#[command(name = "occlusion")]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the CSV data file containing UUIDs and visibility levels
    #[arg(value_name = "DATA_FILE")]
    data_file: String,
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

/// Load the store from a CSV file
fn load_store(data_path: &str) -> Result<ActiveStore> {
    info!(data_file = %data_path, "Loading authorization store");

    let store = load_from_csv(data_path)?;

    info!(uuid_count = store.len(), "Store loaded successfully");

    Ok(store)
}

#[launch]
fn rocket() -> _ {
    // Initialize tracing
    init_tracing();

    // Parse command line arguments
    let args = Args::parse();

    // Load the store
    let store = match load_store(&args.data_file) {
        Ok(store) => store,
        Err(e) => {
            error!(error = %e, "Failed to start server");
            std::process::exit(1);
        }
    };

    info!("Starting Rocket server");

    // Build and launch Rocket server
    rocket::build().manage(store).mount(
        "/",
        routes![
            routes::check,
            routes::check_batch,
            routes::health,
            routes::stats,
        ],
    )
}
