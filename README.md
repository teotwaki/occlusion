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

## Static URL

Bake the data source URL into the binary at compile time:

```bash
OCCLUSION_STATIC_URL="https://example.com/data.csv" \
    cargo build --release --bin server --features static-url
```

The resulting binary requires no arguments and loads data from the compiled-in URL.

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

## Logging

The server uses structured logging via `tracing`. Control log levels with `RUST_LOG`:

```bash
# Default (info level, includes Rocket startup logs)
cargo run --release --bin server -- data.csv

# Suppress Rocket's logs
RUST_LOG=info,rocket=off cargo run --release --bin server -- data.csv

# JSON output for production
cargo run --release --bin server -- data.csv --json-logs
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

## API Usage with HTTPie

### Health Check

```bash
http GET localhost:8000/health
```

### Single Visibility Check

```bash
http POST localhost:8000/api/v1/check \
    'object=550e8400-e29b-41d4-a716-446655440000' \
    'visibility_mask:=10'
```

### Batch Visibility Check

```bash
http POST localhost:8000/api/v1/check/batch \
    'objects:=["550e8400-e29b-41d4-a716-446655440000", "6ba7b810-9dad-11d1-80b4-00c04fd430c8"]' \
    'visibility_mask:=10'
```

### Statistics

```bash
http GET localhost:8000/api/v1/stats
```

### OPA-Compatible Endpoints

```bash
# Single visibility check
http POST localhost:8000/v1/data/occlusion/visible \
    'input[object]=550e8400-e29b-41d4-a716-446655440000' \
    'input[visibility_mask]:=10'

# Batch visibility check
http POST localhost:8000/v1/data/occlusion/visible_batch \
    'input[objects]:=["550e8400-e29b-41d4-a716-446655440000"]' \
    'input[visibility_mask]:=10'
```
