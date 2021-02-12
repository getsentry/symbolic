use std::fmt;
use std::io::Cursor;

use symbolic_common::ByteView;
use symbolic_debuginfo::Object;
use symbolic_demangle::{Demangle, DemangleOptions};
use symbolic_symcache::{SymCache, SymCacheWriter};
use symbolic_testutils::fixture;

type Error = Box<dyn std::error::Error>;

/// Helper to create neat snapshots for symbol tables.
struct FunctionsDebug<'a>(&'a SymCache<'a>);

impl fmt::Debug for FunctionsDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for result in self.0.functions() {
            match result {
                Ok(function) => writeln!(f, "{:>16x} {}", &function.address(), &function.name())?,
                Err(error) => writeln!(f, "{:?}", error)?,
            }
        }

        Ok(())
    }
}

#[test]
fn test_write_header_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("linux/crash.debug"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&object, Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!(symcache, @r###"
    SymCache {
        debug_id: DebugId {
            uuid: "c0bcc3f1-9827-fe65-3058-404b2831d9e6",
            appendix: 0,
        },
        arch: Amd64,
        has_line_info: true,
        has_file_info: true,
        functions: 1964,
    }
    "###);

    Ok(())
}

#[test]
fn test_write_functions_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("linux/crash.debug"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&object, Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!("functions_linux", FunctionsDebug(&symcache));

    Ok(())
}

#[test]
fn test_write_header_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("macos/crash.dSYM/Contents/Resources/DWARF/crash"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&object, Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!(symcache, @r###"
   ⋮SymCache {
   ⋮    debug_id: DebugId {
   ⋮        uuid: "67e9247c-814e-392b-a027-dbde6748fcbf",
   ⋮        appendix: 0,
   ⋮    },
   ⋮    arch: Amd64,
   ⋮    has_line_info: true,
   ⋮    has_file_info: true,
   ⋮    functions: 1863,
   ⋮}
    "###);

    Ok(())
}

#[test]
fn test_write_functions_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("macos/crash.dSYM/Contents/Resources/DWARF/crash"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&object, Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!("functions_macos", FunctionsDebug(&symcache));

    Ok(())
}

#[test]
fn test_write_large_symbol_names() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("regression/large_symbol.sym"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&object, Cursor::new(&mut buffer))?;
    SymCache::parse(&buffer)?;

    Ok(())
}

/// This tests the fix for the bug described in
/// https://github.com/getsentry/symbolic/issues/284#issue-726898083
#[test]
fn test_lookup_no_lines() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("xul.sym"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&object, Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    let symbols = symcache.lookup(0xc6dd98)?.collect::<Vec<_>>()?;

    assert_eq!(symbols.len(), 1);
    let name = symbols[0].function_name();

    assert_eq!(
        name.try_demangle(DemangleOptions::name_only()),
        "std::_Func_impl_no_alloc<`lambda at \
        /builds/worker/checkouts/gecko/netwerk/\
        protocol/http/HttpChannelChild.cpp:411:7',void>::_Do_call()"
    );

    Ok(())
}

/// This tests the fix for the bug described in
/// https://github.com/getsentry/symbolic/issues/284#issuecomment-715587454.
#[test]
fn test_lookup_no_size() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("libgallium_dri.sym"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&object, Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    let symbols = symcache.lookup(0x1489adf)?.collect::<Vec<_>>()?;

    assert_eq!(symbols.len(), 1);
    let name = symbols[0].function_name();

    assert_eq!(
        name.try_demangle(DemangleOptions::name_only()),
        "nouveau_drm_screen_create"
    );

    Ok(())
}
