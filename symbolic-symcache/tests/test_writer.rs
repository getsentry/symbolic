use std::io::Cursor;

use symbolic_common::ByteView;
use symbolic_debuginfo::Object;
use symbolic_symcache::{FunctionsDebug, SymCache, SymCacheConverter};
use symbolic_testutils::fixture;

#[cfg(feature = "il2cpp")]
use symbolic_il2cpp::usym::UsymSymbols;

type Error = Box<dyn std::error::Error>;

#[test]
fn test_write_header_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("linux/crash.debug"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    converter.serialize(&mut Cursor::new(&mut buffer))?;

    #[cfg(target_endian = "little")]
    {
        assert!(buffer.starts_with(b"SYMC"));
    }

    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!(symcache, @r###"
    SymCache {
        version: 7,
        debug_id: DebugId {
            uuid: "c0bcc3f1-9827-fe65-3058-404b2831d9e6",
            appendix: 0,
        },
        arch: Amd64,
        files: 55,
        functions: 697,
        source_locations: 9267,
        ranges: 6846,
        string_bytes: 52180,
    }
    "###);

    Ok(())
}

#[test]
fn test_write_functions_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("linux/crash.debug"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    converter.serialize(&mut Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!("functions_linux", FunctionsDebug(&symcache));

    Ok(())
}

#[test]
fn test_write_header_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("macos/crash.dSYM/Contents/Resources/DWARF/crash"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    converter.serialize(&mut Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!(symcache, @r###"
    SymCache {
        version: 7,
        debug_id: DebugId {
            uuid: "67e9247c-814e-392b-a027-dbde6748fcbf",
            appendix: 0,
        },
        arch: Amd64,
        files: 36,
        functions: 639,
        source_locations: 7781,
        ranges: 5783,
        string_bytes: 42829,
    }
    "###);

    Ok(())
}

#[test]
fn test_write_functions_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("macos/crash.dSYM/Contents/Resources/DWARF/crash"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    converter.serialize(&mut Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!("functions_macos", FunctionsDebug(&symcache));

    Ok(())
}

// Tests that functions with identical names, compilation directories, and languages but different
// entry_pcs have separate, distinct entries in the symcache. The specific use case generating this
// is two identically-named static C functions nestled in two different files sharing a common
// compilation directory. See the overlapping_funcs directory in sentry-testutils for related files.
#[test]
fn test_write_functions_overlapping_funcs() -> Result<(), Error> {
    let buffer = ByteView::open(fixture(
        "macos/overlapping_funcs.dSYM/Contents/Resources/DWARF/overlapping_funcs",
    ))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    converter.serialize(&mut Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!("overlapping_funcs", FunctionsDebug(&symcache));

    Ok(())
}

#[test]
fn test_write_large_symbol_names() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("regression/large_symbol.sym"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    converter.serialize(&mut Cursor::new(&mut buffer))?;
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
    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    converter.serialize(&mut Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    let symbols = symcache.lookup(0xc6dd98).collect::<Vec<_>>();

    assert_eq!(symbols.len(), 1);
    let name = symbols[0].function().name();

    assert_eq!(
        name,
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
    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    converter.serialize(&mut Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    let symbols = symcache.lookup(0x1489adf).collect::<Vec<_>>();

    assert_eq!(symbols.len(), 1);
    let name = symbols[0].function().name();

    assert_eq!(name, "nouveau_drm_screen_create");

    Ok(())
}

/// This tests the fix for the bug described in
/// https://github.com/getsentry/symbolic/issues/285.
#[test]
fn test_lookup_modulo_u16() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("xul2.sym"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    converter.serialize(&mut Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    let symbols = symcache.lookup(0x3c105a1).collect::<Vec<_>>();

    assert_eq!(symbols.len(), 1);
    let name = symbols[0].function().name();

    assert_eq!(name, "Interpret(JSContext*, js::RunState&)");

    Ok(())
}

/// Tests that the cache is lenient toward adding additional flags at the end.
#[test]
fn test_trailing_marker() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("macos/crash.dSYM/Contents/Resources/DWARF/crash"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    converter.serialize(&mut Cursor::new(&mut buffer))?;
    buffer.extend(b"WITH_SYMBOLMAP");

    SymCache::parse(&buffer)?;

    Ok(())
}

#[cfg(feature = "il2cpp")]
#[test]
fn test_mapless_usym() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("il2cpp/artificial.usym"))?;
    let usym = UsymSymbols::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_usym(&usym)?;
    converter.serialize(&mut Cursor::new(&mut buffer))?;

    let cache = SymCache::parse(&buffer)?;

    insta::assert_debug_snapshot!(cache, @r###"
    SymCache {
        version: 7,
        debug_id: DebugId {
            uuid: "153d10d1-0db0-33d6-aacd-a4e1948da97b",
            appendix: 0,
        },
        arch: Arm64,
        files: 0,
        functions: 0,
        source_locations: 0,
        ranges: 0,
        string_bytes: 0,
    }
    "###);

    Ok(())
}

#[cfg(feature = "il2cpp")]
#[test]
fn test_usym() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("il2cpp/managed.usym"))?;
    let usym = UsymSymbols::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_usym(&usym)?;
    converter.serialize(&mut Cursor::new(&mut buffer))?;
    let cache = SymCache::parse(&buffer)?;

    insta::assert_debug_snapshot!(cache, @r###"
    SymCache {
        version: 7,
        debug_id: DebugId {
            uuid: "153d10d1-0db0-33d6-aacd-a4e1948da97b",
            appendix: 0,
        },
        arch: Arm64,
        files: 1,
        functions: 3,
        source_locations: 3,
        ranges: 3,
        string_bytes: 128,
    }
    "###);

    Ok(())
}

#[cfg(feature = "il2cpp")]
#[test]
fn test_write_functions_usym() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("il2cpp/managed.usym"))?;
    let usym = UsymSymbols::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_usym(&usym)?;
    converter.serialize(&mut Cursor::new(&mut buffer))?;
    let cache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!("functions_usym", FunctionsDebug(&cache));

    Ok(())
}
