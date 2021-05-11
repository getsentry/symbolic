use std::fs::File;
use std::io::{BufRead, BufReader};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use symbolic_common::ByteView;
use symbolic_minidump::cfi::CfiCache;
use symbolic_minidump::processor::{FrameInfoMap, ProcessState};
use symbolic_testutils::fixture;

pub fn minidump_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("Internal Minidump");
    let buffer = ByteView::open(fixture("linux/mini.dmp")).unwrap();
    let mut frame_info = FrameInfoMap::new();

    let cfi_records = {
        let file = BufReader::new(File::open(fixture("linux/crash.sym")).unwrap());

        file.lines()
            .skip(169) // STACK CFI records start at line 170
            .map(|l| l.unwrap())
            .collect::<Vec<String>>()
            .join("\n")
    };
    let view = ByteView::from_slice(&cfi_records.as_bytes());

    frame_info.insert(
        "C0BCC3F19827FE653058404B2831D9E60".parse().unwrap(),
        CfiCache::from_bytes(view).unwrap(),
    );

    group.bench_with_input(
        BenchmarkId::new("from_minidump", "linux/mini.dmp & linux/crash.sym"),
        &(&buffer, &frame_info),
        |b, (buffer, frame_info)| b.iter(|| ProcessState::from_minidump(buffer, Some(frame_info))),
    );

    group.bench_with_input(
        BenchmarkId::new("from_minidump_new", "linux/mini.dmp & linux/crash.sym"),
        &(&buffer, &frame_info),
        |b, (buffer, frame_info)| {
            b.iter(|| ProcessState::from_minidump_new(buffer, Some(frame_info)))
        },
    );

    group.finish()
}

criterion_group!(benches, minidump_benchmark);
criterion_main!(benches);
