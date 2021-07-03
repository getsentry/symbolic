use std::ops::Range;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use rand::{seq::SliceRandom, SeedableRng};

use symbolic_minidump::processor::NestedRangeMap;

fn create_ranges(range: Range<u32>, rng: &mut rand::rngs::SmallRng) -> Vec<Range<u32>> {
    let mut ranges = Vec::new();
    go(range, &mut ranges);
    ranges.shuffle(rng);

    ranges
}

fn go(range: Range<u32>, acc: &mut Vec<Range<u32>>) {
    let mid = (range.end - range.start) / 2;
    if mid > range.start + 1 {
        go(range.start..mid, acc);
    }
    if range.start > mid + 1 {
        go(mid..range.end, acc);
    }

    acc.push(range);
}

pub fn nested_range_map_benchmark(c: &mut Criterion) {
    let mut rng = rand::rngs::SmallRng::seed_from_u64(0);
    let ranges = create_ranges(0..10_000, &mut rng);
    c.bench_function("NestedRangeMap insertions", |b| {
        b.iter_batched(
            || ranges.clone(),
            |ranges| {
                let mut map = NestedRangeMap::default();
                for (i, range) in ranges.into_iter().enumerate() {
                    map.insert(range, i);
                }
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, nested_range_map_benchmark);
criterion_main!(benches);
