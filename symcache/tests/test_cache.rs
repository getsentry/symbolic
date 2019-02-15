use std::fmt::Write;

use failure::Error;

use symbolic_common::ByteView;
use symbolic_symcache::SymCache;
use symbolic_testutils::{assert_snapshot, assert_snapshot_plain, fixture_path};

struct Shim<'a>(symbolic_common::Name<'a>);

impl std::fmt::Display for Shim<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use symbolic_common::Language;
        use symbolic_demangle::Demangle;
        let demangled = self.0.try_demangle(Default::default());

        match self.0.language() {
            Language::Unknown => f.write_str(&demangled),
            language => write!(f, "{} [{}]", demangled, language),
        }
    }
}

fn get_functions(symcache: &SymCache<'_>) -> String {
    let mut s = String::new();
    for func in symcache.functions() {
        let func = func.expect("Could not read symcache functions");
        writeln!(s, "{:>16x} {}", func.address(), Shim(func.name()))
            .expect("Could not format symcache function");
    }
    s
}

#[test]
fn test_load_header_linux() {
    let buffer = ByteView::open(fixture_path("symcache/current/linux.symc"))
        .expect("Could not open symcache");
    let symcache = SymCache::parse(&buffer).expect("Could not load symcache");
    assert_snapshot("header_linux.txt", &symcache);
}

#[test]
fn test_load_functions_linux() {
    let buffer = ByteView::open(fixture_path("symcache/current/linux.symc"))
        .expect("Could not open symcache");
    let symcache = SymCache::parse(&buffer).expect("Could not load symcache");
    let functions = get_functions(&symcache);
    assert_snapshot_plain("functions_linux.txt", &functions);
}

#[test]
fn test_load_header_macos() {
    let buffer = ByteView::open(fixture_path("symcache/current/macos.symc"))
        .expect("Could not open symcache");
    let symcache = SymCache::parse(&buffer).expect("Could not load symcache");
    assert_snapshot("header_macos.txt", &symcache);
}

#[test]
fn test_load_functions_macos() {
    let buffer = ByteView::open(fixture_path("symcache/current/macos.symc"))
        .expect("Could not open symcache");
    let symcache = SymCache::parse(&buffer).expect("Could not load symcache");
    let functions = get_functions(&symcache);
    assert_snapshot_plain("functions_macos.txt", &functions);
}

#[test]
fn test_lookup() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("symcache/current/macos.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    let line_infos = symcache.lookup(4_458_187_797 - 4_458_131_456)?;
    assert_snapshot("lookup.txt", &line_infos);

    Ok(())
}
