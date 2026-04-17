use std::io::Cursor;

use symbolic_common::ByteView;
use symbolic_debuginfo::Object;
use symbolic_symcache::{FunctionsDebug, SymCache, SymCacheConverter};
use symbolic_testutils::fixture;

type Error = Box<dyn std::error::Error>;

#[test]
fn test_load_header_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/linux.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!(symcache, @r#"
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
    "#);
    Ok(())
}

#[test]
fn test_load_functions_linux() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/linux.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!("functions_linux", FunctionsDebug(&symcache));
    Ok(())
}

#[test]
fn test_load_header_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/macos.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!(symcache, @r#"
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
    "#);
    Ok(())
}

#[test]
fn test_load_functions_macos() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/macos.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    insta::assert_debug_snapshot!("functions_macos", FunctionsDebug(&symcache));
    Ok(())
}

#[test]
fn test_lookup() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("symcache/current/macos.symc"))?;
    let symcache = SymCache::parse(&buffer)?;
    let source_locations = symcache.lookup(4_458_187_797 - 4_458_131_456);
    let result: Vec<_> = source_locations
        .map(|sl| {
            (
                sl.file().map(|file| file.full_path()).unwrap(),
                sl.line(),
                sl.function(),
            )
        })
        .collect();
    insta::assert_debug_snapshot!("lookup", result);

    Ok(())
}

#[test]
fn test_pdb_srcsrv_remapping() -> Result<(), Error> {
    let buffer = ByteView::open(fixture("windows/crash_with_srcsrv.pdb"))?;
    let object = Object::parse(&buffer)?;

    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    let mut buffer = Vec::new();
    converter.serialize(&mut Cursor::new(&mut buffer))?;

    let cache = SymCache::parse(&buffer)?;

    let file = cache.lookup(0x1000).next().unwrap().file().unwrap();
    assert_eq!(
        file.full_srcsrv_path().as_deref(),
        Some("depot/breakpad/src/client/windows/crash_generation/crash_generation_client.cc")
    );
    assert_eq!(file.srcsrv_revision(), Some("12345"));

    Ok(())
}

#[test]
fn test_backward_compatibility_v7_v8_v9() -> Result<(), Error> {
    let v7_buffer = ByteView::open(fixture("symcache/current/linux.symc"))?;
    let v7_cache = SymCache::parse(&v7_buffer)?;
    assert_eq!(v7_cache.version(), 7);

    for file in v7_cache.files() {
        assert!(file.srcsrv_revision().is_none());
    }

    let regular_pdb_buffer = ByteView::open(fixture("windows/crash.pdb"))?;
    let regular_object = Object::parse(&regular_pdb_buffer)?;

    let mut converter_no_revision = SymCacheConverter::new();
    converter_no_revision.process_object(&regular_object)?;
    let mut v9_no_revision_buffer = Vec::new();
    converter_no_revision.serialize(&mut Cursor::new(&mut v9_no_revision_buffer))?;

    let v9_no_revision_cache = SymCache::parse(&v9_no_revision_buffer)?;
    assert_eq!(v9_no_revision_cache.version(), 9,);

    for file in v9_no_revision_cache.files() {
        assert!(file.srcsrv_revision().is_none());
    }

    let pdb_buffer = ByteView::open(fixture("windows/crash_with_srcsrv.pdb"))?;
    let object = Object::parse(&pdb_buffer)?;

    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    let mut v9_buffer = Vec::new();
    converter.serialize(&mut Cursor::new(&mut v9_buffer))?;

    let v9_cache = SymCache::parse(&v9_buffer)?;
    assert_eq!(v9_cache.version(), 9, "Should create v9 symcache");

    // Verify v9 has both files with and without revisions
    assert!(v9_cache.files().any(|f| f.srcsrv_revision().is_some()));
    assert!(v9_cache.files().any(|f| f.srcsrv_revision().is_none()));

    Ok(())
}
