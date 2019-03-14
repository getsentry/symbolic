#![cfg(feature = "bench")]
#![feature(test)]

extern crate test;

use std::io::Cursor;

use symbolic_common::ByteView;
use symbolic_debuginfo::Object;
use symbolic_symcache::SymCacheWriter;

#[bench]
fn bench_write_linux(b: &mut test::Bencher) {
    b.iter(|| {
        let buffer = ByteView::open("../testutils/fixtures/linux/crash.debug").expect("open");
        let object = Object::parse(&buffer).expect("parse");
        SymCacheWriter::write_object(&object, Cursor::new(Vec::new()))
            .expect("write_object")
            .into_inner()
    });
}

#[bench]
fn bench_write_macos(b: &mut test::Bencher) {
    b.iter(|| {
        let buffer =
            ByteView::open("../testutils/fixtures/macos/crash.dSYM/Contents/Resources/DWARF/crash")
                .expect("open");
        let object = Object::parse(&buffer).expect("parse");
        SymCacheWriter::write_object(&object, Cursor::new(Vec::new()))
            .expect("write_object")
            .into_inner()
    });
}

#[bench]
fn bench_write_breakpad(b: &mut test::Bencher) {
    b.iter(|| {
        let buffer = ByteView::open("../testutils/fixtures/windows/crash.sym").expect("open");
        let object = Object::parse(&buffer).expect("parse");
        SymCacheWriter::write_object(&object, Cursor::new(Vec::new()))
            .expect("write_object")
            .into_inner()
    });
}
