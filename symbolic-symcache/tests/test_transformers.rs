use std::borrow::Cow;
use std::io::Cursor;

use symbolic_common::ByteView;
use symbolic_debuginfo::macho::BcSymbolMap;
use symbolic_debuginfo::Object;
use symbolic_symcache::transform::{File, SourceLocation};
use symbolic_symcache::{SymCache, SymCacheConverter};
use symbolic_testutils::fixture;

type Error = Box<dyn std::error::Error>;

// Test helpers for constructing File and SourceLocation
fn test_file<'s>(
    name: &'s str,
    directory: Option<&'s str>,
    comp_dir: Option<&'s str>,
) -> File<'s> {
    File {
        name: Cow::Borrowed(name),
        directory: directory.map(Cow::Borrowed),
        comp_dir: comp_dir.map(Cow::Borrowed),
    }
}

fn test_source_location<'s>(file: File<'s>, line: u32) -> SourceLocation<'s> {
    SourceLocation { file, line }
}

#[test]
fn test_transformer_symbolmap() -> Result<(), Error> {
    let buffer = ByteView::open(
        "../symbolic-debuginfo/tests/fixtures/2d10c42f-591d-3265-b147-78ba0868073f.dwarf-hidden",
    )?;
    let object = Object::parse(&buffer)?;

    let map_buffer = ByteView::open(
        "../symbolic-debuginfo/tests/fixtures/c8374b6d-6e96-34d8-ae38-efaa5fec424f.bcsymbolmap",
    )?;
    let bc_symbol_map = BcSymbolMap::parse(&map_buffer)?;

    let mut converter = SymCacheConverter::new();
    converter.add_transformer(bc_symbol_map);
    converter.process_object(&object)?;
    let mut buffer = Vec::new();
    converter.serialize(&mut Cursor::new(&mut buffer))?;

    let cache = SymCache::parse(&buffer)?;

    let sl = cache.lookup(0x5a74).next().unwrap();

    assert_eq!(sl.function().name(), "-[SentryMessage initWithFormatted:]");
    assert_eq!(
        sl.file().unwrap().full_path(),
        "/Users/philipphofmann/git-repos/sentry-cocoa/Sources/Sentry/SentryMessage.m"
    );

    Ok(())
}

#[test]
fn test_perforce_transformation() {
    use symbolic_symcache::transform::perforce::PerforcePathMapper;
    use symbolic_symcache::transform::Transformer;

    let srcsrv_data = r#"
SRCSRV: ini ------------------------------------------------
VERSION=1
VERCTRL=Perforce
SRCSRV: source files ---------------------------------------
C:\build\game\src\main.cpp*P4_EDGE*depot/game/src/main.cpp*42
SRCSRV: end ------------------------------------------------
"#;

    let mut mapper =
        PerforcePathMapper::from_srcsrv_data(srcsrv_data).expect("Failed to parse SRCSRV data");

    // Create a source location that matches the SRCSRV entry
    let file = test_file("main.cpp", Some("src"), Some("C:/build/game"));
    let source_loc = test_source_location(file, 10);

    // Transform it
    let transformed = mapper.transform_source_location(source_loc);

    // Verify transformation
    assert_eq!(transformed.file.name, "//depot/game/src/main.cpp");
    assert_eq!(transformed.file.directory, None);
    assert_eq!(
        transformed.file.comp_dir,
        Some(Cow::Borrowed("revision:42"))
    );
    assert_eq!(transformed.line, 10);
}

#[test]
fn test_perforce_no_match() {
    use symbolic_symcache::transform::perforce::PerforcePathMapper;
    use symbolic_symcache::transform::Transformer;

    let srcsrv_data = r#"
SRCSRV: ini ------------------------------------------------
VERSION=1
VERCTRL=Perforce
SRCSRV: source files ---------------------------------------
C:\build\game\src\main.cpp*P4_EDGE*depot/game/src/main.cpp*42
SRCSRV: end ------------------------------------------------
"#;

    let mut mapper =
        PerforcePathMapper::from_srcsrv_data(srcsrv_data).expect("Failed to parse SRCSRV data");

    // Create a source location that doesn't match
    let file = test_file("unknown.cpp", Some("other"), Some("D:/different/path"));
    let source_loc = test_source_location(file, 5);

    // Transform it
    let transformed = mapper.transform_source_location(source_loc);

    // Verify no transformation occurred
    assert_eq!(transformed.file.name, "unknown.cpp");
    assert_eq!(transformed.file.directory, Some(Cow::Borrowed("other")));
    assert_eq!(
        transformed.file.comp_dir,
        Some(Cow::Borrowed("D:/different/path"))
    );
    assert_eq!(transformed.line, 5);
}

#[test]
fn test_perforce_rejects_non_perforce_srcsrv() {
    use symbolic_symcache::transform::perforce::PerforcePathMapper;

    // SRCSRV data with VERCTRL=Git (not Perforce)
    let git_srcsrv_data = r#"
SRCSRV: ini ------------------------------------------------
VERSION=1
VERCTRL=Git
SRCSRV: variables ------------------------------------------
SRCSRV: source files ---------------------------------------
C:\build\game\src\main.cpp*gitserver*repo/src/main.cpp*abc123
SRCSRV: end ------------------------------------------------
"#;

    // Should return None because VERCTRL is not Perforce
    let result = PerforcePathMapper::from_srcsrv_data(git_srcsrv_data);
    assert!(
        result.is_none(),
        "PerforcePathMapper should reject non-Perforce SRCSRV data"
    );

    // Test with missing VERCTRL
    let no_verctrl_data = r#"
SRCSRV: ini ------------------------------------------------
VERSION=1
SRCSRV: source files ---------------------------------------
C:\build\game\src\main.cpp*server*repo/src/main.cpp*123
SRCSRV: end ------------------------------------------------
"#;

    let result = PerforcePathMapper::from_srcsrv_data(no_verctrl_data);
    assert!(
        result.is_none(),
        "PerforcePathMapper should reject SRCSRV data without VERCTRL=Perforce"
    );
}

// End-to-end integration test: PDB with SRCSRV → SymCache with transformed paths
#[test]
fn test_perforce_e2e_with_pdb_and_symcache() -> Result<(), Error> {
    use symbolic_debuginfo::pdb::PdbObject;
    use symbolic_symcache::transform::perforce::PerforcePathMapper;

    // This test requires a PDB with SRCSRV data
    let view = ByteView::open(fixture("windows/crash_with_srcsrv.pdb"))?;
    let pdb = PdbObject::parse(&view)?;

    // Extract raw SRCSRV data from PDB
    let srcsrv_bytes = pdb
        .source_server_data()?
        .expect("crash_with_srcsrv.pdb should have SRCSRV data.");

    let srcsrv_data = std::str::from_utf8(&srcsrv_bytes)?;

    // Create Perforce transformer
    let mapper = PerforcePathMapper::from_srcsrv_data(srcsrv_data)
        .expect("Should be able to parse SRCSRV data into PerforcePathMapper");

    // Parse the PDB as an Object for SymCacheConverter
    let object = Object::parse(&view)?;

    // Create SymCache with Perforce transformer
    let mut converter = SymCacheConverter::new();
    converter.add_transformer(mapper);
    converter.process_object(&object)?;

    let mut buffer = Vec::new();
    converter.serialize(&mut Cursor::new(&mut buffer))?;

    // Parse the generated SymCache
    let cache = SymCache::parse(&buffer)?;

    // Verify that paths have been transformed
    // Find a source location and check it has Perforce depot path format
    // Note: On Windows, paths may use backslashes (\\depot\) instead of forward slashes (//depot/)
    let mut found_depot_path = false;
    for addr in 0..0x10000u64 {
        if let Some(sl) = cache.lookup(addr).next() {
            if let Some(file) = sl.file() {
                let path = file.full_path();
                // Check if path starts with // or \\ (Perforce depot path)
                if path.starts_with("//depot/") || path.starts_with("//")
                    || path.starts_with("\\\\depot\\") || path.starts_with("\\\\") {
                    found_depot_path = true;
                    break;
                }
            }
        }
    }

    assert!(
        found_depot_path,
        "Expected to find at least one file path transformed to Perforce depot format (//depot/... or \\\\depot\\...)"
    );

    Ok(())
}
