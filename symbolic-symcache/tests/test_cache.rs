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
    // Test that PDB with SRCSRV data has remapped paths in the symcache
    let buffer = ByteView::open(fixture("windows/crash_with_srcsrv.pdb"))?;
    let object = Object::parse(&buffer)?;

    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    let mut buffer = Vec::new();
    converter.serialize(&mut Cursor::new(&mut buffer))?;

    let cache = SymCache::parse(&buffer)?;

    // Expected specific path based on the SRCSRV data in the test PDB
    let expected_path =
        "depot/breakpad/src/client/windows/crash_generation/crash_generation_client.cc@12345";

    // Collect all file paths from symcache lookups
    let mut files: Vec<String> = Vec::new();
    for addr in 0..0x100000 {
        if let Some(sl) = cache.lookup(addr).next() {
            if let Some(file) = sl.file() {
                let path = file.full_path();
                if !files.contains(&path) {
                    files.push(path);
                }
            }
        }
    }

    // Verify the exact expected path exists
    assert!(
        files.iter().any(|f| f == expected_path),
        "Expected to find remapped path in symcache: {}. Found paths: {:?}",
        expected_path,
        &files[..files.len().min(10)]
    );

    Ok(())
}
