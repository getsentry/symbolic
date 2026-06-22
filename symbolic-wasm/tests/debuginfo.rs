use symbolic_wasm::debuginfo::Archive;

mod common;

#[test]
#[wasm_bindgen_test::wasm_bindgen_test]
fn test_archive() {
    let data = common::fixture("symbolic-testutils/fixtures/linux/crash.debug");

    let archive = Archive::new(&data).unwrap();

    assert_eq!(archive.file_format(), "elf");
    assert_eq!(archive.object_count(), 1);
}

#[test]
#[wasm_bindgen_test::wasm_bindgen_test]
fn test_archive_zstd() {
    let data = common::fixture("symbolic-testutils/fixtures/linux/crash.debug-zstd");

    let archive = Archive::new(&data).unwrap();

    assert_eq!(archive.file_format(), "elf");
    assert_eq!(archive.object_count(), 1);
}

#[test]
#[wasm_bindgen_test::wasm_bindgen_test]
fn test_archive_zlib() {
    let data = common::fixture("symbolic-testutils/fixtures/linux/crash.debug-zlib");

    let archive = Archive::new(&data).unwrap();

    assert_eq!(archive.file_format(), "elf");
    assert_eq!(archive.object_count(), 1);
}

#[test]
#[wasm_bindgen_test::wasm_bindgen_test]
fn test_archive_objects() {
    let data = common::fixture("symbolic-testutils/fixtures/linux/crash.debug");

    let archive = Archive::new(&data).unwrap();
    let mut objects = archive.objects().unwrap();
    assert_eq!(objects.len(), 1);
    let object = objects.remove(0);

    assert_eq!(&object.debug_id(), "c0bcc3f1-9827-fe65-3058-404b2831d9e6");
    assert_eq!(
        object.code_id().as_deref(),
        Some("f1c3bcc0279865fe3058404b2831d9e64135386c")
    );
    assert_eq!(&object.arch(), "x86_64");
    assert_eq!(&object.file_format(), "elf");
    assert_eq!(&object.kind(), "dbg");
    assert!(object.has_debug_info());
    assert!(!object.has_unwind_info());
    assert!(object.has_symbols());
    assert!(!object.has_sources());
}
