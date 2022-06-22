use std::io::Cursor;

use criterion::{criterion_group, criterion_main, Criterion};

use symbolic_common::ByteView;
use symbolic_debuginfo::Object;
use symbolic_symcache::SymCacheConverter;
use symbolic_testutils::fixture;

fn bench_write_linux(c: &mut Criterion) {
    c.bench_function("write_linux", |b| {
        let buffer = ByteView::open(fixture("linux/crash.debug")).expect("open");
        b.iter(|| {
            let object = Object::parse(&buffer).expect("parse");
            let mut converter = SymCacheConverter::new();
            converter.process_object(&object).expect("process_object");
            converter
                .serialize(&mut Cursor::new(Vec::new()))
                .expect("write_object")
        });
    });
}

fn bench_write_macos(c: &mut Criterion) {
    c.bench_function("write_macos", |b| {
        let buffer = ByteView::open(fixture("macos/crash.dSYM/Contents/Resources/DWARF/crash"))
            .expect("open");

        b.iter(|| {
            let object = Object::parse(&buffer).expect("parse");
            let mut converter = SymCacheConverter::new();
            converter.process_object(&object).expect("process_object");
            converter
                .serialize(&mut Cursor::new(Vec::new()))
                .expect("write_object")
        });
    });
}

fn bench_write_breakpad(c: &mut Criterion) {
    c.bench_function("write_breakpad", |b| {
        let buffer = ByteView::open(fixture("windows/crash.sym")).expect("open");
        b.iter(|| {
            let object = Object::parse(&buffer).expect("parse");
            let mut converter = SymCacheConverter::new();
            converter.process_object(&object).expect("process_object");
            converter
                .serialize(&mut Cursor::new(Vec::new()))
                .expect("write_object")
        });
    });
}

criterion_group!(
    bench_writer,
    bench_write_linux,
    bench_write_macos,
    bench_write_breakpad
);

criterion_main!(bench_writer);
