#![cfg(feature = "wasm-sourcemap")]

use std::io::Cursor;

use symbolic_common::ByteView;
use symbolic_debuginfo::wasm::WasmObject;
use symbolic_symcache::wasm_sourcemap::{
    WasmSourceMapError, WasmSourceMapStats, process_wasm_sourcemap,
};
use symbolic_symcache::{SymCache, SymCacheConverter};
use symbolic_testutils::fixture;

type Error = Box<dyn std::error::Error>;

// Offsets of the three function bodies in `simple.wasm`. See the fixture's `build.py`.
const ADD: u64 = 0x21; // 5 bytes, fully mapped
const MID: u64 = 0x27; // 9 bytes, mapped with a hole in the middle
const TAIL: u64 = 0x31; // 3 bytes, not mapped at all

fn convert(map: &str) -> Result<(Vec<u8>, WasmSourceMapStats), Error> {
    let wasm = ByteView::open(fixture("wasm/sourcemap/simple.wasm"))?;
    let object = WasmObject::parse(&wasm)?;
    let sourcemap = ByteView::open(fixture(map))?;

    let mut converter = SymCacheConverter::new();
    let stats = process_wasm_sourcemap(&mut converter, &object, &sourcemap)?;

    let mut buffer = Vec::new();
    converter.serialize(&mut Cursor::new(&mut buffer))?;
    Ok((buffer, stats))
}

/// Resolves an address to `(function, file, line)`, or `None` if it maps nowhere.
fn lookup(symcache: &SymCache<'_>, addr: u64) -> Option<(String, Option<String>, u32)> {
    let location = symcache.lookup(addr).next()?;
    Some((
        location.function().name().to_owned(),
        location.file().map(|file| file.full_path()),
        location.line(),
    ))
}

#[test]
fn test_function_bodies_are_exact() -> Result<(), Error> {
    let wasm = ByteView::open(fixture("wasm/sourcemap/simple.wasm"))?;
    let object = WasmObject::parse(&wasm)?;

    let bodies: Vec<_> = object
        .function_bodies()
        .map(|body| (body.index, body.address, body.size, body.name))
        .collect();

    // One imported function shifts the wasm indices, and the sizes stop at the end of each body
    // rather than stretching into the next one.
    assert_eq!(
        bodies,
        vec![
            (1, ADD, 5, Some("add")),
            (2, MID, 9, None),
            (3, TAIL, 3, Some("main")),
        ]
    );

    Ok(())
}

#[test]
fn test_mapped_addresses_resolve() -> Result<(), Error> {
    let (buffer, _) = convert("wasm/sourcemap/simple.wasm.map")?;
    let symcache = SymCache::parse(&buffer)?;

    let add = |line| Some(("add".into(), Some("src/main.c".into()), line));
    assert_eq!(lookup(&symcache, ADD), add(10));
    // The first record covers everything up to the next mapping.
    assert_eq!(lookup(&symcache, ADD + 1), add(10));
    assert_eq!(lookup(&symcache, ADD + 2), add(11));
    assert_eq!(lookup(&symcache, ADD + 4), add(11));

    Ok(())
}

#[test]
fn test_padding_between_bodies_resolves_to_nothing() -> Result<(), Error> {
    let (buffer, _) = convert("wasm/sourcemap/simple.wasm.map")?;
    let symcache = SymCache::parse(&buffer)?;

    // The byte after `add` holds the size prefix of the next body. Without the boundary guard it
    // would inherit `add`'s last line.
    assert_eq!(lookup(&symcache, ADD + 5), None);
    // Same past the end of the last body.
    assert_eq!(lookup(&symcache, TAIL + 3), None);

    Ok(())
}

#[test]
fn test_unmapped_regions_keep_the_function_but_drop_the_line() -> Result<(), Error> {
    let (buffer, _) = convert("wasm/sourcemap/simple.wasm.map")?;
    let symcache = SymCache::parse(&buffer)?;

    // The name section does not name this body, so the name is synthesized from the wasm index.
    let unnamed = "wasm-function[2]".to_owned();
    assert_eq!(
        lookup(&symcache, MID),
        Some((unnamed.clone(), Some("src/main.c".into()), 20))
    );

    // A one field terminator marks `MID + 4` onwards as unmapped. The function survives, the line
    // does not, and the range does not swallow the next mapping.
    assert_eq!(lookup(&symcache, MID + 4), Some((unnamed.clone(), None, 0)));
    assert_eq!(lookup(&symcache, MID + 5), Some((unnamed.clone(), None, 0)));
    assert_eq!(
        lookup(&symcache, MID + 6),
        Some((unnamed, Some("src/main.c".into()), 21))
    );

    // `main` has no mappings at all, so only its name is known.
    assert_eq!(lookup(&symcache, TAIL), Some(("main".into(), None, 0)));
    assert_eq!(lookup(&symcache, TAIL + 2), Some(("main".into(), None, 0)));

    Ok(())
}

#[test]
fn test_stats_report_coverage() -> Result<(), Error> {
    let (_, stats) = convert("wasm/sourcemap/simple.wasm.map")?;

    assert_eq!(
        stats,
        WasmSourceMapStats {
            functions_total: 3,
            functions_mapped: 2,
            tokens_used: 4,
            // The map ends with a mapping past the end of the module.
            tokens_out_of_range: 1,
            tokens_unmapped: 1,
        }
    );

    Ok(())
}

#[test]
fn test_five_field_segments() -> Result<(), Error> {
    // Emscripten 4.0.2x emitted a fifth field while leaving `names` empty. The decoder rejects the
    // whole map, so this has to surface as an actionable error rather than as an empty cache.
    let error = convert("wasm/sourcemap/five_field.wasm.map").unwrap_err();
    assert_eq!(
        error.to_string(),
        "source map references undefined name #0; rebuild with a newer emscripten"
    );

    // The same shape decodes fine once `names` is populated.
    let (buffer, stats) = convert("wasm/sourcemap/five_field_named.wasm.map")?;
    assert_eq!(stats.tokens_used, 1);
    let symcache = SymCache::parse(&buffer)?;
    assert_eq!(
        lookup(&symcache, ADD),
        Some(("add".into(), Some("src/main.c".into()), 10))
    );

    Ok(())
}

#[test]
fn test_rejects_javascript_source_map() -> Result<(), Error> {
    let wasm = ByteView::open(fixture("wasm/sourcemap/simple.wasm"))?;
    let object = WasmObject::parse(&wasm)?;

    // Anything mapping a second destination line is a regular JS map, where columns are columns and
    // not addresses.
    let sourcemap = br#"{"version":3,"sources":["a.js"],"names":[],"mappings":"AAAA;AACA"}"#;

    let mut converter = SymCacheConverter::new();
    let error = process_wasm_sourcemap(&mut converter, &object, sourcemap).unwrap_err();
    assert!(matches!(error, WasmSourceMapError::NotWasm(1)));

    Ok(())
}
