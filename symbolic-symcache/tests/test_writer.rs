use std::io::Cursor;

use symbolic_common::ByteView;
use symbolic_debuginfo::Object;
use symbolic_symcache::{FunctionsDebug, SymCache, SymCacheConverter};
use symbolic_testutils::fixture;

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
        source_locations: 8305,
        ranges: 6843,
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
        source_locations: 7204,
        ranges: 5759,
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

/// This tests the fix for the bug described in
/// https://github.com/getsentry/symbolic/issues/646.
#[test]
fn test_lookup_second_line_in_inlinee() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("macos/crash.dSYM/Contents/Resources/DWARF/crash"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    converter.serialize(&mut Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;

    // Test an address at the second line of an inlinee.
    let symbols = symcache.lookup(0xd7d).collect::<Vec<_>>();
    assert_eq!(symbols.len(), 2);
    let name = symbols[1].function().name();
    assert_eq!(name, "_ZN15google_breakpad18MinidumpFileWriterD2Ev");

    // Test an address where an inlinee's caller has its second line.
    let symbols = symcache.lookup(0x2dd4).collect::<Vec<_>>();
    assert_eq!(symbols.len(), 5);
    let name = symbols[4].function().name();
    assert_eq!(
        name,
        "_ZN15google_breakpad13DynamicImages18GetExecutableImageEv"
    );

    Ok(())
}

/// This tests the fix for the bug described in
/// https://github.com/getsentry/symbolic/issues/647.
#[test]
fn test_lookup_gap_inlinee() -> Result<(), Error> {
    use symbolic_common::{Language, Name, NameMangling};
    use symbolic_debuginfo::{FileInfo, Function, LineInfo};
    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();

    // Manually add a Function of the right shape. A function like this is theoretically possible
    // to encounter in a PDB file, but it requires the call to inlineeA and inlineeB to be on the
    // same line.
    // The interesting aspect about this function is that inlineeB has a gap, inlineeA sits in that
    // gap, and the "parent line" which covers inlineeB's first chunk also covers the gap.

    converter.process_symbolic_function(&Function {
        address: 0x1000,
        size: 0x30,
        name: Name::new("outer", NameMangling::Unmangled, Language::Rust),
        compilation_dir: b"",
        lines: vec![LineInfo {
            address: 0x1000,
            size: Some(0x30),
            file: FileInfo {
                name: b"main.rs",
                dir: b"",
            },
            line: 5,
        }],
        inlinees: vec![
            Function {
                address: 0x1010,
                size: 0x10,
                name: Name::new("inlineeA", NameMangling::Unmangled, Language::Rust),
                compilation_dir: b"",
                lines: vec![LineInfo {
                    address: 0x1010,
                    size: Some(0x10),
                    file: FileInfo {
                        name: b"main.rs",
                        dir: b"",
                    },
                    line: 20,
                }],
                inlinees: vec![],
                inline: true,
            },
            Function {
                address: 0x1000,
                size: 0x30,
                name: Name::new("inlineeB", NameMangling::Unmangled, Language::Rust),
                compilation_dir: b"",
                lines: vec![
                    LineInfo {
                        address: 0x1000,
                        size: Some(0x10),
                        file: FileInfo {
                            name: b"main.rs",
                            dir: b"",
                        },
                        line: 40,
                    },
                    LineInfo {
                        address: 0x1020,
                        size: Some(0x10),
                        file: FileInfo {
                            name: b"main.rs",
                            dir: b"",
                        },
                        line: 42,
                    },
                ],
                inlinees: vec![],
                inline: true,
            },
        ],
        inline: false,
    });
    converter.serialize(&mut buffer)?;
    let symcache = SymCache::parse(&buffer)?;

    // Test that inlineeA at address 0x1010 was not overwritten by the "remaining parent line"
    // after inlineeB's first line was processed.
    let symbols = symcache.lookup(0x1010).collect::<Vec<_>>();
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].function().name(), "inlineeA");
    assert_eq!(symbols[1].function().name(), "outer");

    Ok(())
}

#[test]
fn test_undecorate_windows_symbols() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("windows/crash.pdb"))?;
    let object = Object::parse(&buffer)?;

    let mut buffer = Vec::new();
    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    converter.serialize(&mut Cursor::new(&mut buffer))?;
    let symcache = SymCache::parse(&buffer)?;

    let symbols = symcache.lookup(0x3756).collect::<Vec<_>>();
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].function().name(), "malloc");

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
