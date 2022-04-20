use std::fmt;
use std::io::Cursor;

use symbolic_common::{ByteView, SelfCell};
use symbolic_debuginfo::macho::BcSymbolMap;
use symbolic_debuginfo::Object;
use symbolic_symcache::transform::{self, Transformer};
use symbolic_symcache::{SymCache, SymCacheWriter};
use symbolic_testutils::fixture;

#[cfg(feature = "il2cpp")]
use symbolic_il2cpp::usym::UsymSymbols;

type Error = Box<dyn std::error::Error>;

/// Helper to create neat snapshots for symbol tables.
struct FunctionsDebug<'a>(&'a SymCache<'a>);

#[allow(deprecated)]
impl fmt::Debug for FunctionsDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut vec: Vec<_> = self
            .0
            .functions()
            .filter_map(|f| match f {
                Ok(f) => {
                    if f.address() != u32::MAX as u64 {
                        Some(f)
                    } else {
                        None
                    }
                }
                Err(_) => None,
            })
            .collect();

        vec.sort_by_key(|f| f.address());
        for function in vec {
            writeln!(f, "{:>16x} {}", &function.address(), &function.name())?;
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
        source_locations: 8236,
        ranges: 6762,
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
    SymCache {
        version: 7,
        debug_id: DebugId {
            uuid: "67e9247c-814e-392b-a027-dbde6748fcbf",
            appendix: 0,
        },
        arch: Amd64,
        files: 36,
        functions: 639,
        source_locations: 6033,
        ranges: 4591,
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
    SymCacheWriter::write_object(&object, Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    let symbols = symcache.lookup(0x1489adf)?.collect::<Vec<_>>()?;

    assert_eq!(symbols.len(), 1);
    let name = symbols[0].function_name();

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
    SymCacheWriter::write_object(&object, Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;
    let symbols = symcache.lookup(0x3c105a1)?.collect::<Vec<_>>()?;

    assert_eq!(symbols.len(), 1);
    let name = symbols[0].function_name();

    assert_eq!(name, "Interpret(JSContext*, js::RunState&)");

    Ok(())
}

/// Tests that the cache is lenient toward adding additional flags at the end.
#[test]
fn test_trailing_marker() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("macos/crash.dSYM/Contents/Resources/DWARF/crash"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    SymCacheWriter::write_object(&object, Cursor::new(&mut buffer))?;
    buffer.extend(b"WITH_SYMBOLMAP");

    SymCache::parse(&buffer)?;

    Ok(())
}

// FIXME: This is a huge pain, can't this be simpler somehow?
struct OwnedBcSymbolMap(SelfCell<ByteView<'static>, BcSymbolMap<'static>>);

impl Transformer for OwnedBcSymbolMap {
    fn transform_function<'f>(&'f self, f: transform::Function<'f>) -> transform::Function<'f> {
        self.0.get().transform_function(f)
    }

    fn transform_source_location<'f>(
        &'f self,
        sl: transform::SourceLocation<'f>,
    ) -> transform::SourceLocation<'f> {
        self.0.get().transform_source_location(sl)
    }
}

#[test]
fn test_transformer_symbolmap() -> Result<(), Error> {
    let buffer = ByteView::open(
        "../symbolic-debuginfo/tests/fixtures/2d10c42f-591d-3265-b147-78ba0868073f.dwarf-hidden",
    )?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut writer = SymCacheWriter::new(Cursor::new(&mut buffer))?;

    let map_buffer = ByteView::open(
        "../symbolic-debuginfo/tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap",
    )?;
    let bc_symbol_map = OwnedBcSymbolMap(SelfCell::try_new(map_buffer, |s| unsafe {
        BcSymbolMap::parse(&*s)
    })?);

    writer.add_transformer(bc_symbol_map);

    writer.process_object(&object)?;

    let _ = writer.finish()?;
    let cache = SymCache::parse(&buffer)?;

    let sl = cache.lookup(0x5a74)?.next().unwrap()?;

    assert_eq!(sl.function_name(), "-[SentryMessage initWithFormatted:]");
    assert_eq!(
        sl.abs_path(),
        "/Users/philipphofmann/git-repos/sentry-cocoa/Sources/Sentry/SentryMessage.m"
    );

    Ok(())
}

#[cfg(feature = "il2cpp")]
#[test]
fn test_mapless_usym() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("il2cpp/artificial.usym"))?;
    let usym = UsymSymbols::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut writer = SymCacheWriter::new(Cursor::new(&mut buffer))?;

    writer.process_usym(&usym)?;

    let _ = writer.finish()?;
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
    let mut writer = SymCacheWriter::new(Cursor::new(&mut buffer))?;

    writer.process_usym(&usym)?;

    let _ = writer.finish()?;
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
    let mut writer = SymCacheWriter::new(Cursor::new(&mut buffer))?;

    writer.process_usym(&usym)?;

    let _ = writer.finish()?;
    let cache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!("functions_usym", FunctionsDebug(&cache));

    Ok(())
}
