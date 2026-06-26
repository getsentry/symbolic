//! Integration tests for `ObjectLineMapping::from_object_with_provider`.
//!
//! These exercise the filesystem-free provider path against a real object,
//! supplying source file contents from memory instead of disk. The native
//! `from_object` (filesystem) variant shares the same code path.

use symbolic_common::ByteView;
use symbolic_debuginfo::Object;
use symbolic_il2cpp::ObjectLineMapping;
use symbolic_testutils::fixture;

/// Synthetic Il2cpp C++: a `source_info` marker followed by a code line maps the
/// generated C++ line 2 to `Game.cs` line 42.
const SYNTHETIC_SOURCE: &[u8] = b"//<source_info:Game.cs:42>\nint generated = 0;\n";

/// The provider is invoked for the object's referenced source files, and files
/// that contain `source_info` markers contribute to the serialized mapping.
#[test]
fn from_object_with_provider_parses_source_info() {
    let view = ByteView::open(fixture("windows/Sentry.Samples.Console.Basic.pdb")).unwrap();
    let object = Object::parse(&view).unwrap();

    let mut calls = 0usize;
    let mapping = ObjectLineMapping::from_object_with_provider(&object, |path| {
        assert!(!path.is_empty(), "expected a non-empty source path");
        calls += 1;
        Some(SYNTHETIC_SOURCE.to_vec())
    })
    .unwrap();
    assert!(
        calls > 0,
        "provider should be called for referenced source files"
    );

    let mut buf = Vec::new();
    let written = mapping.to_writer(&mut buf).unwrap();
    assert!(written, "mapping should be reported as non-empty");

    let json = String::from_utf8(buf).unwrap();
    assert!(
        json.contains("\"__debug-id__\""),
        "missing debug-id sentinel: {json}"
    );
    assert!(json.contains("\"Game.cs\""), "missing C# file: {json}");
    assert!(json.contains("\"2\":42"), "missing line mapping: {json}");
}

/// When the provider yields nothing, no mapping is produced and `to_writer`
/// reports the mapping as empty.
#[test]
fn from_object_with_provider_empty_without_sources() {
    let view = ByteView::open(fixture("windows/Sentry.Samples.Console.Basic.pdb")).unwrap();
    let object = Object::parse(&view).unwrap();

    let mapping =
        ObjectLineMapping::from_object_with_provider(&object, |_path| None::<Vec<u8>>).unwrap();

    let mut buf = Vec::new();
    let written = mapping.to_writer(&mut buf).unwrap();
    assert!(
        !written,
        "mapping should be empty when the provider yields no sources"
    );
}
