use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use symbolic_common::ByteView;
use symbolic_debuginfo::breakpad::{BreakpadObject, BreakpadStackRecord};
use symbolic_testutils::fixture;

pub fn breakpad_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("Breakpad parser benchmarks");

    for file in ["linux/crash.sym", "macos/crash.sym", "windows/crash.sym"].iter() {
        let view = ByteView::open(fixture(file)).unwrap();
        let object: BreakpadObject = BreakpadObject::parse(&view).unwrap();

        group.bench_with_input(
            BenchmarkId::new("info records", file),
            &object,
            |b, object| {
                b.iter(|| {
                    for record in object.info_records() {
                        record.unwrap();
                    }
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("func and line records", file),
            &object,
            |b, object| {
                b.iter(|| {
                    for record in object.func_records() {
                        for line in record.unwrap().lines() {
                            line.unwrap();
                        }
                    }
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("public records", file),
            &object,
            |b, object| {
                b.iter(|| {
                    for record in object.public_records() {
                        record.unwrap();
                    }
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("file records", file),
            &object,
            |b, object| {
                b.iter(|| {
                    for record in object.file_records() {
                        record.unwrap();
                    }
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("stack records", file),
            &object,
            |b, object| {
                b.iter(|| {
                    for record in object.stack_records() {
                        if let BreakpadStackRecord::Cfi(cfi_record) = record.unwrap() {
                            for delta in cfi_record.deltas() {
                                delta.unwrap();
                            }
                        }
                    }
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, breakpad_parser);
criterion_main!(benches);
