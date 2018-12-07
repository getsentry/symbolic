use std::fmt::Write;

use symbolic_common::byteview::ByteView;
use symbolic_debuginfo::FatObject;
use symbolic_symcache::SymCache;
use symbolic_testutils::{assert_snapshot, assert_snapshot_plain, fixture_path};

fn get_functions(symcache: &SymCache<'_>) -> String {
    let mut s = String::new();
    for func in symcache.functions() {
        let func = func.expect("Could not read symcache functions");
        writeln!(s, "{:>16x} {:#}", func.addr(), func).expect("Could not format symcache function");
    }
    s
}

#[test]
fn test_write_header_linux() {
    let buffer = ByteView::from_path(fixture_path("linux/crash.debug"))
        .expect("Could not open the ELF file");
    let fat = FatObject::parse(buffer).expect("Could not create an object");
    let object = fat
        .get_object(0)
        .expect("Could not get the first object")
        .expect("Missing object");

    let symcache = SymCache::from_object(&object).expect("Could not generate symcache");
    assert_snapshot("header_linux.txt", &symcache);
}

#[test]
fn test_write_functions_linux() {
    let buffer = ByteView::from_path(fixture_path("linux/crash.debug"))
        .expect("Could not open the ELF file");
    let fat = FatObject::parse(buffer).expect("Could not create an object");
    let object = fat
        .get_object(0)
        .expect("Could not get the first object")
        .expect("Missing object");

    let symcache = SymCache::from_object(&object).expect("Could not generate symcache");
    let functions = get_functions(&symcache);
    assert_snapshot_plain("functions_linux.txt", &functions);
}

#[test]
fn test_write_header_macos() {
    let buffer = ByteView::from_path(fixture_path(
        "macos/crash.dSYM/Contents/Resources/DWARF/crash",
    ))
    .expect("Could not open the dSYM file");
    let fat = FatObject::parse(buffer).expect("Could not create an object");
    let object = fat
        .get_object(0)
        .expect("Could not get the first object")
        .expect("Missing object");

    let symcache = SymCache::from_object(&object).expect("Could not generate symcache");
    assert_snapshot("header_macos.txt", &symcache);
}

#[test]
fn test_write_functions_macos() {
    let buffer = ByteView::from_path(fixture_path(
        "macos/crash.dSYM/Contents/Resources/DWARF/crash",
    ))
    .expect("Could not open the dSYM file");
    let fat = FatObject::parse(buffer).expect("Could not create an object");
    let object = fat
        .get_object(0)
        .expect("Could not get the first object")
        .expect("Missing object");

    let symcache = SymCache::from_object(&object).expect("Could not generate symcache");
    let functions = get_functions(&symcache);
    assert_snapshot_plain("functions_macos.txt", &functions);
}

#[test]
fn test_write_large_symbol_names() {
    let buffer = ByteView::from_path(fixture_path("regression/large_symbol.sym"))
        .expect("Could not open breakpad sym");
    let fat = FatObject::parse(buffer).expect("Could not create an object");
    let object = fat
        .get_object(0)
        .expect("Could not get the first object")
        .expect("Missing object");

    SymCache::from_object(&object).expect("Failed to process large symbol name");
}
