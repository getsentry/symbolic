use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::fs::File;
use std::io::{BufRead, BufReader};
use symbolic_common::ByteView;
use symbolic_minidump::cfi::CfiCache;
use symbolic_minidump::processor::{FrameInfoMap, ProcessState};
use symbolic_testutils::fixture;

pub fn minidump_benchmark(c: &mut Criterion) {
    let buffer = ByteView::open(fixture("linux/mini.dmp")).unwrap();
    let mut frame_info = FrameInfoMap::new();

    let cfi_records = {
        let file = BufReader::new(File::open(fixture("linux/crash.sym")).unwrap());

        // Read STACK CFI records starting at line 170
        file.lines()
            .skip(169)
            .map(|l| l.unwrap())
            .collect::<Vec<String>>()
            .join("\n")
    };
    let view = ByteView::from_slice(&cfi_records.as_bytes());

    frame_info.insert(
        "C0BCC3F19827FE653058404B2831D9E60".parse().unwrap(),
        CfiCache::from_bytes(view).unwrap(),
    );

    c.bench_with_input(
        BenchmarkId::new("from_minidump", "linux/mini.dmp & linux/crash.sym"),
        &(buffer, Some(frame_info)),
        |b, (buffer, frame_info)| {
            b.iter(|| ProcessState::from_minidump(buffer, frame_info.as_ref()))
        },
    );
}

criterion_group!(benches, minidump_benchmark);
criterion_main!(benches);
