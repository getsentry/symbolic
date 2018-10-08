extern crate symbolic_common;
extern crate symbolic_debuginfo;
extern crate symbolic_testutils;

use symbolic_common::byteview::ByteView;
use symbolic_debuginfo::{DebugFeatures, FatObject, ObjectFeature};
use symbolic_testutils::fixture_path;

#[test]
fn test_features_elf_bin() {
    let buffer = ByteView::from_path(fixture_path("linux/crash")).expect("Could not open file");
    let fat = FatObject::parse(buffer).expect("Could not create an object");
    let object = fat
        .get_object(0)
        .expect("Could not get the first object")
        .expect("Missing object");

    assert_eq!(
        object.features(),
        [ObjectFeature::UnwindInfo].iter().cloned().collect()
    );
}

#[test]
fn test_features_elf_dbg() {
    let buffer =
        ByteView::from_path(fixture_path("linux/crash.debug")).expect("Could not open file");
    let fat = FatObject::parse(buffer).expect("Could not create an object");
    let object = fat
        .get_object(0)
        .expect("Could not get the first object")
        .expect("Missing object");

    assert_eq!(
        object.features(),
        [ObjectFeature::DebugInfo].iter().cloned().collect()
    );
}

#[test]
fn test_features_mach_bin() {
    let buffer = ByteView::from_path(fixture_path("macos/crash")).expect("Could not open file");
    let fat = FatObject::parse(buffer).expect("Could not create an object");
    let object = fat
        .get_object(0)
        .expect("Could not get the first object")
        .expect("Missing object");

    assert_eq!(
        object.features(),
        [ObjectFeature::UnwindInfo].iter().cloned().collect()
    );
}

#[test]
fn test_features_mach_dbg() {
    let buffer = ByteView::from_path(fixture_path(
        "macos/crash.dSYM/Contents/Resources/DWARF/crash",
    )).expect("Could not open file");
    let fat = FatObject::parse(buffer).expect("Could not create an object");
    let object = fat
        .get_object(0)
        .expect("Could not get the first object")
        .expect("Missing object");

    assert_eq!(
        object.features(),
        [ObjectFeature::DebugInfo].iter().cloned().collect()
    );
}

#[test]
fn test_features_breakpad() {
    let buffer = ByteView::from_path(fixture_path("macos/crash.sym")).expect("Could not open file");
    let fat = FatObject::parse(buffer).expect("Could not create an object");
    let object = fat
        .get_object(0)
        .expect("Could not get the first object")
        .expect("Missing object");

    assert_eq!(
        object.features(),
        [ObjectFeature::DebugInfo, ObjectFeature::UnwindInfo]
            .iter()
            .cloned()
            .collect()
    );
}
