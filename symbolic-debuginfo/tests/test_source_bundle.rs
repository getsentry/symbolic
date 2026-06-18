//! Tests for building source bundles from caller-supplied source content via
//! [`SourceBundleWriter::write_object_with_source_provider`].

use std::collections::{HashMap, HashSet};
use std::io::Cursor;

use symbolic_common::ByteView;
use symbolic_debuginfo::sourcebundle::SourceBundleWriter;
use symbolic_debuginfo::Object;
use symbolic_testutils::fixture;

type Error = Box<dyn std::error::Error>;

/// Collect the non-virtual source paths a debug object references.
fn referenced_sources(object: &Object) -> Result<Vec<String>, Error> {
    let session = object.debug_session()?;
    let mut paths = Vec::new();
    for file in session.files() {
        let path = file?.abs_path_str();
        if path.starts_with('<') && path.ends_with('>') {
            continue;
        }
        paths.push(path);
    }
    Ok(paths)
}

/// `write_object_with_source_provider` bundles source content supplied by the
/// caller instead of reading from the filesystem, so it works even when none of
/// the original build-time source files exist on disk.
#[test]
fn test_write_object_with_source_provider() -> Result<(), Error> {
    let view = ByteView::open(fixture("linux/crash.debug"))?;
    let object = Object::parse(&view)?;

    let referenced = referenced_sources(&object)?;
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
    let writer = SourceBundleWriter::start(&mut cursor)?;
    let written = writer.write_object_with_source_provider(&object, "crash.debug", |path| {
        provided
            .contains(path)
            .then(|| format!("// synthetic source for {path}\n").into_bytes())
    })?;
    assert!(written, "bundle should contain at least one source file");

    // Re-parse the bundle and confirm the supplied source round-trips.
    let bundle_view = ByteView::from_vec(cursor.into_inner());
    let bundle = Object::parse(&bundle_view)?;
    assert_eq!(bundle.debug_id(), object.debug_id());
    assert!(bundle.has_sources());

    let bundle_session = bundle.debug_session()?;
    let descriptor = bundle_session
        .source_by_path(&pick)?
        .expect("supplied source should be present in the bundle");
    assert_eq!(descriptor.contents(), Some(expected.as_str()));

    assert!(
        bundle_session.source_by_path(&unreferenced)?.is_none(),
        "a provided but unreferenced path must not be bundled"
    );

    Ok(())
}

/// A provider that returns `None` for everything yields an empty bundle.
#[test]
fn test_write_object_with_source_provider_no_sources() -> Result<(), Error> {
    let view = ByteView::open(fixture("linux/crash.debug"))?;
    let object = Object::parse(&view)?;

    let mut cursor = Cursor::new(Vec::new());
    let writer = SourceBundleWriter::start(&mut cursor)?;
    let written = writer.write_object_with_source_provider(&object, "crash.debug", |_| None)?;
    assert!(!written, "no sources provided should yield an empty bundle");

    Ok(())
}

/// Source enumeration + provider bundling also works for zstd-compressed DWARF
/// debug sections (decompressed natively via the C `zstd` library; via the
/// pure-Rust `ruzstd` decoder on wasm).
#[test]
fn test_write_object_with_source_provider_zstd() -> Result<(), Error> {
    let view = ByteView::open(fixture("linux/crash.debug-zstd"))?;
    let object = Object::parse(&view)?;

    let referenced = referenced_sources(&object)?;
    assert!(!referenced.is_empty());

    let provided: HashSet<String> = referenced.iter().cloned().collect();
    let mut cursor = Cursor::new(Vec::new());
    let writer = SourceBundleWriter::start(&mut cursor)?;
    let written =
        writer.write_object_with_source_provider(&object, "crash.debug-zstd", |path| {
            provided.contains(path).then(|| b"x\n".to_vec())
        })?;
    assert!(written);

    Ok(())
}

/// Regression test for the il2cpp + destructive-provider case: a source file
/// referenced via an il2cpp `//<source_info:...>` comment is collected in a
/// second pass. Because each path is requested at most once, a destructive
/// provider (`HashMap::remove`) must still bundle it (not silently drop it).
#[test]
fn test_write_object_with_source_provider_il2cpp_destructive() -> Result<(), Error> {
    let view = ByteView::open(fixture("linux/crash.debug"))?;
    let object = Object::parse(&view)?;

    let referenced = referenced_sources(&object)?;
    let main_path = referenced
        .first()
        .cloned()
        .expect("fixture should reference a source file");
    let cs_path = "/il2cpp/Referenced.cs".to_string();

    // The main source carries an il2cpp reference to the C# file, which is
    // discovered during the first pass and bundled in the second pass.
    let mut contents: HashMap<String, Vec<u8>> = HashMap::new();
    contents.insert(
        main_path.clone(),
        format!("int main() {{}}\n//<source_info:{cs_path}:1>\n").into_bytes(),
    );
    contents.insert(cs_path.clone(), b"// csharp source\n".to_vec());

    let mut cursor = Cursor::new(Vec::new());
    let mut writer = SourceBundleWriter::start(&mut cursor)?;
    writer.collect_il2cpp_sources(true);
    // Destructive provider — the exact pattern Seer flagged.
    let written = writer
        .write_object_with_source_provider(&object, "crash.debug", |path| contents.remove(path))?;
    assert!(written);

    let bundle_view = ByteView::from_vec(cursor.into_inner());
    let bundle = Object::parse(&bundle_view)?;
    let session = bundle.debug_session()?;
    assert!(
        session.source_by_path(&main_path)?.is_some(),
        "main source must be bundled"
    );
    assert!(
        session.source_by_path(&cs_path)?.is_some(),
        "il2cpp-referenced source must not be dropped by a destructive provider"
    );

    Ok(())
}
