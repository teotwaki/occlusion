//! Generate CSV test data for the occlusion server.

use clap::Parser;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::{
    fs::File,
    io::{self, BufWriter, Write},
};
use uuid::Uuid;

/// Generate CSV test data for the occlusion server
#[derive(Parser, Debug)]
#[command(name = "generate-csv")]
#[command(version, about, long_about = None)]
struct Args {
    /// Number of rows (UUIDs) to generate
    #[arg(value_name = "ROWS")]
    rows: usize,

    /// Number of visibility levels to use (1-256)
    #[arg(value_name = "LEVELS")]
    levels: u16,

    /// Output file (default: stdout)
    #[arg(short, long, default_value = "-")]
    output: String,

    /// Random seed for reproducibility
    #[arg(short, long)]
    seed: Option<u64>,

    /// Use skewed distribution (80% at level 0)
    #[arg(long)]
    skewed: bool,
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    if args.levels < 1 || args.levels > 256 {
        eprintln!("Error: levels must be between 1 and 256");
        std::process::exit(1);
    }

    // Initialize RNG
    let mut rng = match args.seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_os_rng(),
    };

    // Determine output destination
    let mut writer: BufWriter<Box<dyn Write>> = if args.output == "-" {
        BufWriter::new(Box::new(io::stdout()))
    } else {
        BufWriter::new(Box::new(File::create(&args.output)?))
    };

    // Write header
    writeln!(writer, "uuid,visibility_level")?;

    // Generate rows
    for _ in 0..args.rows {
        let uid = Uuid::new_v4();

        let level: u8 = if args.skewed {
            // 80% chance of level 0, 20% spread across other levels
            if rng.random::<f32>() < 0.8 {
                0
            } else if args.levels > 1 {
                rng.random_range(1..args.levels) as u8
            } else {
                0
            }
        } else {
            // Uniform distribution across levels
            rng.random_range(0..args.levels) as u8
        };

        writeln!(writer, "{},{}", uid, level)?;
    }

    writer.flush()?;
    Ok(())
}
