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

    // Expected specific path and revision based on the SRCSRV data in the test PDB
    let expected_path =
        "depot/breakpad/src/client/windows/crash_generation/crash_generation_client.cc";
    let expected_revision = "12345";

    // Find the remapped file in symcache lookups
    let mut found_expected = false;
    for addr in 0..0x100000 {
        if let Some(sl) = cache.lookup(addr).next() {
            if let Some(file) = sl.file() {
                let path = file.full_path();
                if path == expected_path {
                    // Verify the revision is set correctly
                    let revision = file.revision();
                    assert_eq!(
                        revision,
                        Some(expected_revision),
                        "Expected revision '{}' for remapped file in symcache, found: {:?}",
                        expected_revision,
                        revision
                    );
                    found_expected = true;
                    break;
                }
            }
        }
    }

    assert!(
        found_expected,
        "Expected to find remapped path with revision in symcache: {} with revision {}",
        expected_path, expected_revision
    );

    Ok(())
}

#[test]
fn test_backward_compatibility_v7_v8_v9() -> Result<(), Error> {
    // Test that we can read v7, v8 (with 12-byte File structs), and v9 (with 16-byte File structs)
    // This ensures backward compatibility across format changes.

    // Part 1: Load a v7 symcache from fixtures to verify we can read old formats
    let v7_buffer = ByteView::open(fixture("symcache/current/linux.symc"))?;
    let v7_cache = SymCache::parse(&v7_buffer)?;
    assert_eq!(v7_cache.version(), 7, "Should load v7 symcache");

    // Verify we can read files from v7 (no revision field, will be set to None)
    let mut v7_file_count = 0;
    for file in v7_cache.files() {
        assert_eq!(
            file.revision(),
            None,
            "v7 files should have no revision (converted to None)"
        );
        v7_file_count += 1;
        if v7_file_count >= 5 {
            break; // Just check a few files
        }
    }
    assert!(v7_file_count > 0, "v7 should have files");

    // Verify v7 lookups work
    let mut v7_lookups_work = false;
    for addr in 0..0x100000 {
        if let Some(sl) = v7_cache.lookup(addr).next() {
            if let Some(file) = sl.file() {
                // Verify revision is None for v7 files
                assert_eq!(file.revision(), None);
                v7_lookups_work = true;
                break;
            }
        }
    }
    assert!(v7_lookups_work, "v7 symcache lookups should work");

    // Part 2: Create a v9 symcache without SRCSRV (simulates v8 behavior)
    // This tests that objects without revision data work in v9 format
    let regular_pdb_buffer = ByteView::open(fixture("windows/crash.pdb"))?;
    let regular_object = Object::parse(&regular_pdb_buffer)?;

    let mut converter_no_revision = SymCacheConverter::new();
    converter_no_revision.process_object(&regular_object)?;
    let mut v9_no_revision_buffer = Vec::new();
    converter_no_revision.serialize(&mut Cursor::new(&mut v9_no_revision_buffer))?;

    let v9_no_revision_cache = SymCache::parse(&v9_no_revision_buffer)?;
    assert_eq!(
        v9_no_revision_cache.version(),
        9,
        "Should create v9 symcache even without revisions"
    );

    // All files should have revision = None (like v8 files converted to v9)
    let mut all_none = true;
    let mut file_count = 0;
    for file in v9_no_revision_cache.files() {
        if file.revision().is_some() {
            all_none = false;
            break;
        }
        file_count += 1;
        if file_count >= 10 {
            break;
        }
    }
    assert!(
        all_none,
        "v9 symcache without SRCSRV should have all revisions as None"
    );

    // Part 3: Create a v9 symcache WITH revision data using PDB with SRCSRV
    let pdb_buffer = ByteView::open(fixture("windows/crash_with_srcsrv.pdb"))?;
    let object = Object::parse(&pdb_buffer)?;

    let mut converter = SymCacheConverter::new();
    converter.process_object(&object)?;
    let mut v9_buffer = Vec::new();
    converter.serialize(&mut Cursor::new(&mut v9_buffer))?;

    let v9_cache = SymCache::parse(&v9_buffer)?;
    assert_eq!(v9_cache.version(), 9, "Should create v9 symcache");

    // Verify v9 has both files with and without revisions
    let mut found_with_revision = false;
    let mut found_without_revision = false;

    for file in v9_cache.files() {
        if file.revision().is_some() {
            found_with_revision = true;
        } else {
            found_without_revision = true;
        }

        if found_with_revision && found_without_revision {
            break;
        }
    }

    assert!(
        found_with_revision,
        "v9 symcache should contain at least one file with revision (from SRCSRV)"
    );
    assert!(
        found_without_revision,
        "v9 symcache should contain files without revision (non-remapped files)"
    );

    // Verify that v9 can be parsed and lookups work correctly
    let mut lookups_work = false;
    for addr in 0..0x100000 {
        if let Some(sl) = v9_cache.lookup(addr).next() {
            if let Some(_file) = sl.file() {
                lookups_work = true;
                break;
            }
        }
    }

    assert!(lookups_work, "v9 symcache lookups should work");

    Ok(())
}
