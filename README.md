# Occlusion

High-performance Rust authorization server for UUID visibility lookups.

## Overview

Stores millions of UUIDs with hierarchical visibility levels (0-255). A UUID is visible if its
stored level <= the request's visibility mask.

## Quick Start

```bash
# Build
cargo build --release

# Generate test data
cargo run --release --bin generate-csv -- 10000 10 -o data.csv

# Run server
cargo run --release --bin server -- data.csv

# Check server help for all options
cargo run --release --bin server -- --help
```

## Store Implementations

Selected at compile time via feature flags:

| Feature | Store | Best For |
|---------|-------|----------|
| (default) | HashMapStore | General use |
| `vec` | VecStore | Memory-constrained |
| `hybrid` | HybridAuthStore | 80-90% at level 0 |
| `fullhash` | FullHashStore | Worst-case optimization |

```bash
cargo run --release --bin server --features hybrid -- data.csv
```

Run `cargo bench -p occlusion --features bench` for performance comparisons.

## Data Format

CSV with header:

```csv
uuid,visibility_level
550e8400-e29b-41d4-a716-446655440000,8
6ba7b810-9dad-11d1-80b4-00c04fd430c8,15
```

## Generating Test Data

```bash
# Generate to stdout
cargo run --release --bin generate-csv -- 1000 10

# Generate to file with seed for reproducibility
cargo run --release --bin generate-csv -- 1000000 256 -o data.csv --seed 42

# Skewed distribution (80% at level 0)
cargo run --release --bin generate-csv -- 1000000 256 --skewed -o data.csv
```

## Docker

```bash
# Build
docker build -t occlusion .

# Build with alternative store
docker build --build-arg FEATURES="hybrid" -t occlusion .

# Run
docker run -p 8000:8000 -v ./data.csv:/app/data.csv occlusion /app/data.csv
```

## Development

```bash
# Run tests
cargo test

# Run benchmarks
cargo bench -p occlusion --features bench

# Run clippy
cargo clippy
```
