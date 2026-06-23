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

#[test]
#[wasm_bindgen_test::wasm_bindgen_test]
fn test_debug_session_files() {
    let data =
        common::fixture("symbolic-testutils/fixtures/windows/Sentry.Samples.Console.Basic.pdb");

    let archive = Archive::new(&data).unwrap();
    let mut objects = archive.objects().unwrap();
    let object = objects.remove(0);

    let session = object.debug_session().unwrap();
    let files = session.files().unwrap();

    assert!(!files.is_empty());
    assert!(files.iter().all(|file| !file.abs_path_str().is_empty()));
}

#[test]
#[wasm_bindgen_test::wasm_bindgen_test]
fn test_debug_session_source_by_path() {
    let data =
        common::fixture("symbolic-testutils/fixtures/windows/Sentry.Samples.Console.Basic.pdb");

    let archive = Archive::new(&data).unwrap();
    let mut objects = archive.objects().unwrap();
    let object = objects.remove(0);

    let session = object.debug_session().unwrap();
    let first = session.files().unwrap().remove(0).abs_path_str();

    // A referenced path resolves without error (contents may or may not be embedded).
    session.source_by_path(&first).unwrap();

    // A path the object does not reference resolves to nothing.
    assert!(session
        .source_by_path("/definitely/not/a/referenced/source")
        .unwrap()
        .is_none());
}

#[cfg(target_arch = "wasm32")]
mod wasm_only {
    use super::*;

    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use symbolic_wasm::debuginfo::sourcebundle::SourceBundleWriter;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};

    #[wasm_bindgen_test::wasm_bindgen_test]
    fn test_source_bundle_writer_write_object() {
        let object = {
            let data = common::fixture(
                "symbolic-testutils/fixtures/windows/Sentry.Samples.Console.Basic.pdb",
            );
            let archive = Archive::new(&data).unwrap();
            let mut objects = archive.objects().unwrap();
            objects.remove(0)
        };

        let filter_call_count = Rc::new(AtomicUsize::new(0));
        let filter = {
            let filter_call_count = Rc::clone(&filter_call_count);
            Closure::<dyn Fn(JsValue, JsValue) -> bool>::new(move |_, _| -> bool {
                filter_call_count.fetch_add(1, Ordering::Relaxed);
                true
            })
            .into_js_value()
        };

        let provider = Closure::<dyn Fn(String) -> js_sys::Uint8Array>::new(move |path| {
            let source = format!("// synthetic source for {path}\n");
            js_sys::Uint8Array::new_from_slice(source.as_bytes())
        })
        .into_js_value();

        let writer = SourceBundleWriter::new().unwrap();
        let bundle = writer
            .write_object(
                &object,
                "whatever",
                &filter.unchecked_into(),
                &provider.unchecked_into(),
            )
            .unwrap()
            .unwrap();

        assert_eq!(filter_call_count.load(Ordering::Relaxed), 4);

        let bundle_archive = Archive::new(&bundle).unwrap();
        assert_eq!(bundle_archive.file_format(), "sourcebundle");

        let bundle_objects = bundle_archive.objects().unwrap();
        assert_eq!(bundle_objects.len(), 1);
        assert!(bundle_objects[0].has_sources());
    }
}
