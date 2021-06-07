use criterion::{criterion_group, criterion_main, Criterion};

use symbolic_minidump::processor::NestedRangeMap;

pub fn nested_range_map_benchmark(c: &mut Criterion) {
    c.bench_function("NestedRangeMap insertions", |b| {
        b.iter(|| {
            let mut map = NestedRangeMap::default();

            for i in 0..1_000 {
                let left = i * 1_000;
                let right = (i + 1) * 1_000;
                map.insert(left..right, 0);

                for j in 0..1_000 {
                    map.insert(left + j..right, j);
                }
            }
        })
    });
}

criterion_group!(benches, nested_range_map_benchmark);
criterion_main!(benches);
