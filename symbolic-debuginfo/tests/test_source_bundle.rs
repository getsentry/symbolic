//! Tests for building source bundles from caller-supplied source content via
//! [`SourceBundleWriter::write_object_with_source_provider`].

use std::collections::HashSet;
use std::io::Cursor;

use symbolic_common::ByteView;
use symbolic_debuginfo::sourcebundle::SourceBundleWriter;
use symbolic_debuginfo::Object;
use symbolic_testutils::fixture;

/// Collect the non-virtual source paths a debug object references.
fn referenced_sources(object: &Object) -> Vec<String> {
    let session = object.debug_session().unwrap();
    session
        .files()
        .map(|file| file.unwrap().abs_path_str())
        .filter(|path| !(path.starts_with('<') && path.ends_with('>')))
        .collect()
}

/// `write_object_with_source_provider` bundles source content supplied by the
/// caller instead of reading from the filesystem, so it works even when none of
/// the original build-time source files exist on disk.
#[test]
fn test_write_object_with_source_provider() {
    let view = ByteView::open(fixture("linux/crash.debug")).unwrap();
    let object = Object::parse(&view).unwrap();

    let referenced = referenced_sources(&object);
    assert!(
        !referenced.is_empty(),
        "fixture should reference source files"
    );

    let mut provided: HashSet<String> = referenced.iter().cloned().collect();
    // A provided path the object does not reference must never be bundled.
    let unreferenced = "/nonexistent/not-referenced.rs".to_string();
    provided.insert(unreferenced.clone());
    let pick = referenced[0].clone();
    let expected = format!("// synthetic source for {pick}\n");

    let mut cursor = Cursor::new(Vec::new());
    let writer = SourceBundleWriter::start(&mut cursor).unwrap();
    let written = writer
        .write_object_with_source_provider(&object, "crash.debug", |path| {
            provided
                .contains(path)
                .then(|| Cursor::new(format!("// synthetic source for {path}\n").into_bytes()))
        })
        .unwrap();
    assert!(written, "bundle should contain at least one source file");

    // Re-parse the bundle and confirm the supplied source round-trips.
    let bundle_view = ByteView::from_vec(cursor.into_inner());
    let bundle = Object::parse(&bundle_view).unwrap();
    assert_eq!(bundle.debug_id(), object.debug_id());
    assert!(bundle.has_sources());

    let bundle_session = bundle.debug_session().unwrap();
    let descriptor = bundle_session
        .source_by_path(&pick)
        .unwrap()
        .expect("supplied source should be present in the bundle");
    assert_eq!(descriptor.contents(), Some(expected.as_str()));

    assert!(
        bundle_session
            .source_by_path(&unreferenced)
            .unwrap()
            .is_none(),
        "a provided but unreferenced path must not be bundled"
    );
}

/// A provider that returns `None` for everything yields an empty bundle.
#[test]
fn test_write_object_with_source_provider_no_sources() {
    let view = ByteView::open(fixture("linux/crash.debug")).unwrap();
    let object = Object::parse(&view).unwrap();

    let mut cursor = Cursor::new(Vec::new());
    let writer = SourceBundleWriter::start(&mut cursor).unwrap();
    let written = writer
        .write_object_with_source_provider(&object, "crash.debug", |_| None::<&[u8]>)
        .unwrap();
    assert!(!written, "no sources provided should yield an empty bundle");
}

/// Source enumeration + provider bundling also works for zstd-compressed DWARF
/// debug sections (decompressed natively via the C `zstd` library; via the
/// pure-Rust `ruzstd` decoder on wasm).
#[test]
fn test_write_object_with_source_provider_zstd() {
    let view = ByteView::open(fixture("linux/crash.debug-zstd")).unwrap();
    let object = Object::parse(&view).unwrap();

    let referenced = referenced_sources(&object);
    assert!(!referenced.is_empty());

    let provided: HashSet<String> = referenced.iter().cloned().collect();
    let mut cursor = Cursor::new(Vec::new());
    let writer = SourceBundleWriter::start(&mut cursor).unwrap();
    let written = writer
        .write_object_with_source_provider(&object, "crash.debug-zstd", |path| {
            provided.contains(path).then(|| &b"x\n"[..])
        })
        .unwrap();
    assert!(written);
}
