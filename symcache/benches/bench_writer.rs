use std::io::Cursor;

use criterion::{criterion_group, criterion_main, Criterion};

use symbolic_common::ByteView;
use symbolic_debuginfo::Object;
use symbolic_symcache::SymCacheWriter;

fn bench_write_linux(c: &mut Criterion) {
    c.bench_function("write_linux", |b| {
        let buffer = ByteView::open("../testutils/fixtures/linux/crash.debug").expect("open");
        b.iter(|| {
            let object = Object::parse(&buffer).expect("parse");
            SymCacheWriter::write_object(&object, Cursor::new(Vec::new()))
                .expect("write_object")
                .into_inner()
        });
    });
}

fn bench_write_macos(c: &mut Criterion) {
    c.bench_function("write_macos", |b| {
        let buffer =
            ByteView::open("../testutils/fixtures/macos/crash.dSYM/Contents/Resources/DWARF/crash")
                .expect("open");

        b.iter(|| {
            let object = Object::parse(&buffer).expect("parse");
            SymCacheWriter::write_object(&object, Cursor::new(Vec::new()))
                .expect("write_object")
                .into_inner()
        });
    });
}

fn bench_write_breakpad(c: &mut Criterion) {
    c.bench_function("write_breakpad", |b| {
        let buffer = ByteView::open("../testutils/fixtures/windows/crash.sym").expect("open");
        b.iter(|| {
            let object = Object::parse(&buffer).expect("parse");
            SymCacheWriter::write_object(&object, Cursor::new(Vec::new()))
                .expect("write_object")
                .into_inner()
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
