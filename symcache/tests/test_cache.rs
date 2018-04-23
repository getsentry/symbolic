extern crate symbolic_common;
extern crate symbolic_debuginfo;
extern crate symbolic_symcache;
extern crate symbolic_testutils;

use std::fmt::Write;
use symbolic_common::byteview::ByteView;
use symbolic_symcache::SymCache;
use symbolic_testutils::{assert_snapshot, assert_snapshot_plain, fixture_path};

fn get_functions(symcache: &SymCache) -> String {
    let mut s = String::new();
    for func in symcache.functions() {
        let func = func.expect("Could not read symcache functions");
        writeln!(s, "{:>16x} {:#}", func.addr(), func).expect("Could not format symcache function");
    }
    s
}

#[test]
fn test_load_header_linux() {
    let buffer = ByteView::from_path(fixture_path("symcache/current/linux.symc"))
        .expect("Could not open symcache");
    let symcache = SymCache::new(buffer).expect("Could not load symcache");
    assert_snapshot("header_linux.txt", &symcache);
}

#[test]
fn test_load_functions_linux() {
    let buffer = ByteView::from_path(fixture_path("symcache/current/linux.symc"))
        .expect("Could not open symcache");
    let symcache = SymCache::new(buffer).expect("Could not load symcache");
    let functions = get_functions(&symcache);
    assert_snapshot_plain("functions_linux.txt", &functions);
}

#[test]
fn test_load_header_macos() {
    let buffer = ByteView::from_path(fixture_path("symcache/current/macos.symc"))
        .expect("Could not open symcache");
    let symcache = SymCache::new(buffer).expect("Could not load symcache");
    assert_snapshot("header_macos.txt", &symcache);
}

#[test]
fn test_load_functions_macos() {
    let buffer = ByteView::from_path(fixture_path("symcache/current/macos.symc"))
        .expect("Could not open symcache");
    let symcache = SymCache::new(buffer).expect("Could not load symcache");
    let functions = get_functions(&symcache);
    assert_snapshot_plain("functions_macos.txt", &functions);
}

#[test]
fn test_lookup() {
    let buffer = ByteView::from_path(fixture_path("symcache/current/macos.symc"))
        .expect("Could not open symcache");
    let symcache = SymCache::new(buffer).expect("Could not load symcache");
    let line_infos = symcache
        .lookup(4458187797 - 4458131456)
        .expect("Could not lookup");
    assert_snapshot("lookup.txt", &line_infos);
}
