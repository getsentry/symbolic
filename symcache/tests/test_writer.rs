use std::fmt::Write;
use std::io::Cursor;

use failure::Error;

use symbolic_common::ByteView;
use symbolic_debuginfo::Object;
use symbolic_symcache::{SymCache, SymCacheWriter};
use symbolic_testutils::{assert_snapshot, assert_snapshot_plain, fixture_path};

struct Shim<'a>(symbolic_common::Name<'a>);

impl std::fmt::Display for Shim<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use symbolic_demangle::Demangle;
        write!(
            f,
            "{} [{}]",
            self.0.try_demangle(Default::default()),
            self.0.language()
        )
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
fn test_write_header_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("linux/crash.debug"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&object, Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    assert_snapshot("header_linux.txt", &symcache);

    Ok(())
}

#[test]
fn test_write_functions_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("linux/crash.debug"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&object, Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    let functions = get_functions(&symcache);
    assert_snapshot_plain("functions_linux.txt", &functions);

    Ok(())
}

#[test]
fn test_write_header_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path(
        "macos/crash.dSYM/Contents/Resources/DWARF/crash",
    ))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&object, Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    assert_snapshot("header_macos.txt", &symcache);

    Ok(())
}

#[test]
fn test_write_functions_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path(
        "macos/crash.dSYM/Contents/Resources/DWARF/crash",
    ))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&object, Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    let functions = get_functions(&symcache);
    assert_snapshot_plain("functions_macos.txt", &functions);

    Ok(())
}

#[test]
fn test_write_large_symbol_names() -> Result<(), Error> {
    let buffer = ByteView::open(fixture_path("regression/large_symbol.sym"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&object, Cursor::new(&mut buffer))?;
    SymCache::parse(&buffer)?;

    Ok(())
}
