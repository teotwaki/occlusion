//! Benchmarks comparing all store implementations.
//!
//! Run with: `cargo bench -p occlusion --features bench`
//!
//! ## Performance Summary (2M UUIDs, with FxHash)
//!
//! | Implementation | Uniform | Level 0 | Higher | Batch (100) |
//! |----------------|---------|---------|--------|-------------|
//! | HashMapStore | 2.7ns | 2.7ns | 2.8ns | 347ns |
//! | VecStore | 51ns | 51ns | 51ns | 7.9µs |
//! | HybridAuthStore | 53ns | 2.5ns | 48ns | 780ns |
//! | FullHashStore | 12ns | 2.3ns | 21ns | 422ns |
//!
//! ## Key Findings
//!
//! - **HashMapStore** is the fastest and simplest - recommended default
//! - **FxHash** provides 4-5x speedup over std HashMap
//! - **VecStore** uses least memory (~17 bytes/UUID) but is slowest
//! - **HybridAuthStore** excels when 80-90% of UUIDs are at level 0
//! - **FullHashStore** has best worst-case performance for mask=0 queries

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use occlusion::{FullHashStore, HashMapStore, HybridAuthStore, Store, VecStore};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::hint::black_box;
use uuid::Uuid;

// Helper to generate deterministic random UUIDs using a seeded RNG
fn generate_uuids(count: usize) -> Vec<Uuid> {
    let mut rng = StdRng::seed_from_u64(12345); // Fixed seed for reproducibility
    (0..count)
        .map(|_| {
            let bytes: [u8; 16] = rng.random();
            Uuid::from_bytes(bytes)
        })
        .collect()
}

// Uniform distribution benchmarks (for comparison baseline)
fn benchmark_uniform_distribution(c: &mut Criterion) {
    let mut group = c.benchmark_group("uniform_distribution");

    // Create 2M random UUIDs uniformly distributed across 8 levels (0-7)
    let uuids = generate_uuids(2_000_000);
    let entries: Vec<(Uuid, u8)> = uuids
        .iter()
        .enumerate()
        .map(|(i, uuid)| (*uuid, (i % 8) as u8))
        .collect();

    let vec_store = VecStore::new(entries.clone()).unwrap();
    let hybrid_store = HybridAuthStore::new(entries.clone()).unwrap();
    let fullhash_store = FullHashStore::new(entries.clone()).unwrap();
    let hashmap_store = HashMapStore::new(entries).unwrap();

    // Test UUID at level 5 (not in level 0 for hybrid)
    let test_uuid = uuids[5];

    group.bench_function(BenchmarkId::new("vecstore", "is_visible"), |b| {
        b.iter(|| black_box(vec_store.is_visible(black_box(&test_uuid), black_box(7))))
    });

    group.bench_function(BenchmarkId::new("hybrid", "is_visible"), |b| {
        b.iter(|| black_box(hybrid_store.is_visible(black_box(&test_uuid), black_box(7))))
    });

    group.bench_function(BenchmarkId::new("fullhash", "is_visible"), |b| {
        b.iter(|| black_box(fullhash_store.is_visible(black_box(&test_uuid), black_box(7))))
    });

    group.bench_function(BenchmarkId::new("hashmap", "is_visible"), |b| {
        b.iter(|| black_box(hashmap_store.is_visible(black_box(&test_uuid), black_box(7))))
    });

    group.finish();
}

// Skewed distribution benchmarks (90% at level 0)
fn benchmark_skewed_distribution(c: &mut Criterion) {
    let mut group = c.benchmark_group("skewed_distribution");

    // Create 2M random UUIDs: 90% at level 0, 10% at levels 1-7
    let uuids = generate_uuids(2_000_000);
    let entries: Vec<(Uuid, u8)> = uuids
        .iter()
        .enumerate()
        .map(|(i, uuid)| {
            let level = if i < 1_800_000 {
                0
            } else {
                ((i % 7) + 1) as u8
            };
            (*uuid, level)
        })
        .collect();

    let vec_store = VecStore::new(entries.clone()).unwrap();
    let hybrid_store = HybridAuthStore::new(entries.clone()).unwrap();
    let fullhash_store = FullHashStore::new(entries.clone()).unwrap();
    let hashmap_store = HashMapStore::new(entries).unwrap();

    // Test level 0 UUID (90% of queries)
    let level_0_uuid = uuids[100_000];

    group.bench_function(BenchmarkId::new("vecstore", "level_0_lookup"), |b| {
        b.iter(|| black_box(vec_store.is_visible(black_box(&level_0_uuid), black_box(7))))
    });

    group.bench_function(BenchmarkId::new("hybrid", "level_0_lookup"), |b| {
        b.iter(|| black_box(hybrid_store.is_visible(black_box(&level_0_uuid), black_box(7))))
    });

    group.bench_function(BenchmarkId::new("fullhash", "level_0_lookup"), |b| {
        b.iter(|| black_box(fullhash_store.is_visible(black_box(&level_0_uuid), black_box(7))))
    });

    group.bench_function(BenchmarkId::new("hashmap", "level_0_lookup"), |b| {
        b.iter(|| black_box(hashmap_store.is_visible(black_box(&level_0_uuid), black_box(7))))
    });

    // Test higher level UUID (10% of queries)
    let higher_level_uuid = uuids[1_900_000];

    group.bench_function(BenchmarkId::new("vecstore", "higher_level_lookup"), |b| {
        b.iter(|| black_box(vec_store.is_visible(black_box(&higher_level_uuid), black_box(7))))
    });

    group.bench_function(BenchmarkId::new("hybrid", "higher_level_lookup"), |b| {
        b.iter(|| black_box(hybrid_store.is_visible(black_box(&higher_level_uuid), black_box(7))))
    });

    group.bench_function(BenchmarkId::new("fullhash", "higher_level_lookup"), |b| {
        b.iter(|| black_box(fullhash_store.is_visible(black_box(&higher_level_uuid), black_box(7))))
    });

    group.bench_function(BenchmarkId::new("hashmap", "higher_level_lookup"), |b| {
        b.iter(|| black_box(hashmap_store.is_visible(black_box(&higher_level_uuid), black_box(7))))
    });

    group.finish();
}

// Batch query benchmarks
fn benchmark_batch_queries(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_queries");

    // Create 2M random UUIDs: 90% at level 0
    let uuids = generate_uuids(2_000_000);
    let entries: Vec<(Uuid, u8)> = uuids
        .iter()
        .enumerate()
        .map(|(i, uuid)| {
            let level = if i < 1_800_000 {
                0
            } else {
                ((i % 7) + 1) as u8
            };
            (*uuid, level)
        })
        .collect();

    let vec_store = VecStore::new(entries.clone()).unwrap();
    let hybrid_store = HybridAuthStore::new(entries.clone()).unwrap();
    let fullhash_store = FullHashStore::new(entries.clone()).unwrap();
    let hashmap_store = HashMapStore::new(entries).unwrap();

    // Batch of 100 UUIDs (90% from level 0, 10% from higher)
    let batch_uuids: Vec<Uuid> = (0..90)
        .map(|i| uuids[i * 10_000])
        .chain((0..10).map(|i| uuids[1_800_000 + i * 1_000]))
        .collect();

    group.bench_function(BenchmarkId::new("vecstore", "batch_100"), |b| {
        b.iter(|| black_box(vec_store.check_batch(black_box(&batch_uuids), black_box(7))))
    });

    group.bench_function(BenchmarkId::new("hybrid", "batch_100"), |b| {
        b.iter(|| black_box(hybrid_store.check_batch(black_box(&batch_uuids), black_box(7))))
    });

    group.bench_function(BenchmarkId::new("fullhash", "batch_100"), |b| {
        b.iter(|| black_box(fullhash_store.check_batch(black_box(&batch_uuids), black_box(7))))
    });

    group.bench_function(BenchmarkId::new("hashmap", "batch_100"), |b| {
        b.iter(|| black_box(hashmap_store.check_batch(black_box(&batch_uuids), black_box(7))))
    });

    group.finish();
}

// Worst case for hybrid: all lookups at higher levels with mask 0
fn benchmark_worst_case(c: &mut Criterion) {
    let mut group = c.benchmark_group("worst_case");

    let uuids = generate_uuids(2_000_000);
    let entries: Vec<(Uuid, u8)> = uuids
        .iter()
        .enumerate()
        .map(|(i, uuid)| {
            let level = if i < 1_800_000 {
                0
            } else {
                ((i % 7) + 1) as u8
            };
            (*uuid, level)
        })
        .collect();

    let vec_store = VecStore::new(entries.clone()).unwrap();
    let hybrid_store = HybridAuthStore::new(entries.clone()).unwrap();
    let fullhash_store = FullHashStore::new(entries.clone()).unwrap();
    let hashmap_store = HashMapStore::new(entries).unwrap();

    let higher_level_uuid = uuids[1_900_000];

    group.bench_function(BenchmarkId::new("vecstore", "high_level_mask_0"), |b| {
        b.iter(|| black_box(vec_store.is_visible(black_box(&higher_level_uuid), black_box(0))))
    });

    group.bench_function(BenchmarkId::new("hybrid", "high_level_mask_0"), |b| {
        b.iter(|| black_box(hybrid_store.is_visible(black_box(&higher_level_uuid), black_box(0))))
    });

    group.bench_function(BenchmarkId::new("fullhash", "high_level_mask_0"), |b| {
        b.iter(|| black_box(fullhash_store.is_visible(black_box(&higher_level_uuid), black_box(0))))
    });

    group.bench_function(BenchmarkId::new("hashmap", "high_level_mask_0"), |b| {
        b.iter(|| black_box(hashmap_store.is_visible(black_box(&higher_level_uuid), black_box(0))))
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_uniform_distribution,
    benchmark_skewed_distribution,
    benchmark_batch_queries,
    benchmark_worst_case
);
criterion_main!(benches);
