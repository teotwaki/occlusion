#[macro_use]
extern crate rocket;

mod models;
mod routes;

use clap::Parser;
use occlusion::{load_from_csv, load_fullhash_from_csv, load_hashmap_from_csv, load_hybrid_from_csv, Store};
use std::sync::Arc;

/// High-performance authorization server for UUID visibility lookups
#[derive(Parser, Debug)]
#[command(name = "occlusion")]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the CSV data file containing UUIDs and visibility levels
    #[arg(value_name = "DATA_FILE")]
    data_file: String,

    /// Store implementation algorithm
    #[arg(short, long, value_name = "ALGORITHM", default_value = "hashmap")]
    #[arg(value_parser = ["hashmap", "vec", "hybrid", "fullhash"])]
    algorithm: String,
}

#[launch]
fn rocket() -> _ {
    // Parse command line arguments
    let args = Args::parse();
    let data_path = args.data_file;
    let store_type = args.algorithm.to_lowercase();

    // Load the authorization store
    println!("Loading data from: {}", data_path);
    println!("Store type: {}", store_type);

    let store: Arc<dyn Store> = match store_type.as_str() {
        "hashmap" => match load_hashmap_from_csv(&data_path) {
            Ok(store) => {
                println!("✓ Loaded {} UUIDs using HashMap store", store.len());
                Arc::new(store)
            }
            Err(e) => {
                eprintln!("Error loading data: {}", e);
                std::process::exit(1);
            }
        },
        "vec" => match load_from_csv(&data_path) {
            Ok(store) => {
                println!("✓ Loaded {} UUIDs using sorted vector store", store.len());
                Arc::new(store)
            }
            Err(e) => {
                eprintln!("Error loading data: {}", e);
                std::process::exit(1);
            }
        },
        "hybrid" => {
            match load_hybrid_from_csv(&data_path) {
                Ok(store) => {
                    let stats = store.distribution_stats();
                    println!("✓ Loaded {} UUIDs using hybrid store", store.len());
                    println!("  Distribution: {}", stats);

                    // Warn if distribution is not skewed
                    if stats.level_0_percentage < 70.0 {
                        eprintln!(
                            "⚠ Warning: Only {:.1}% of UUIDs at level 0",
                            stats.level_0_percentage
                        );
                        eprintln!("  Hybrid store is optimized for 80-90% at level 0");
                        eprintln!("  Consider using --algorithm hashmap for better performance");
                    }

                    Arc::new(store)
                }
                Err(e) => {
                    eprintln!("Error loading data: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "fullhash" => match load_fullhash_from_csv(&data_path) {
            Ok(store) => {
                let stats = store.distribution_stats();
                println!("✓ Loaded {} UUIDs using full hash store", store.len());
                println!("  Distribution: {}", stats);
                Arc::new(store)
            }
            Err(e) => {
                eprintln!("Error loading data: {}", e);
                std::process::exit(1);
            }
        },
        _ => {
            eprintln!("Invalid algorithm: {}", store_type);
            eprintln!("Valid options: hashmap (default), vec, hybrid, fullhash");
            std::process::exit(1);
        }
    };

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
