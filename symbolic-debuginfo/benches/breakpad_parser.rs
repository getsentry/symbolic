use std::fs::File;
use std::io::{BufReader, Read};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use symbolic_common::ByteView;
use symbolic_debuginfo::breakpad::{BreakpadObject, BreakpadStackRecord};
use symbolic_testutils::fixture;

pub fn breakpad_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("Breakpad parser benchmarks");

    for file in ["linux/crash.sym", "macos/crash.sym", "windows/crash.sym"].iter() {
        let mut buf_reader = BufReader::new(File::open(fixture(file)).unwrap());
        let mut buffer = String::new();
        buf_reader.read_to_string(&mut buffer).unwrap();
        let view = ByteView::from_slice(&buffer.as_bytes());
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
