use symbolic_wasm::debuginfo::Archive;
use symbolic_wasm::il2cpp::il2cpp_line_mapping;

mod common;

// `il2cpp_line_mapping` drives JS callbacks, so it only runs under the
// wasm-bindgen test harness (node/headless), not native `cargo test`.
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};

#[wasm_bindgen_test::wasm_bindgen_test]
fn test_il2cpp_line_mapping() {
    let object = {
        let data =
            common::fixture("symbolic-testutils/fixtures/windows/Sentry.Samples.Console.Basic.pdb");
        let archive = Archive::new(&data).unwrap();
        let mut objects = archive.objects().unwrap();
        objects.remove(0)
    };

    // Synthetic Il2cpp C++: a `source_info` marker followed by a code line
    // maps generated C++ line 2 to `Game.cs` line 42. The provider ignores
    // the path and returns this for every referenced source file.
    let provider = Closure::<dyn Fn(String) -> js_sys::Uint8Array>::new(move |_path| {
        let source = "//<source_info:Game.cs:42>\nint generated = 0;\n";
        js_sys::Uint8Array::new_from_slice(source.as_bytes())
    })
    .into_js_value();

    let bytes = il2cpp_line_mapping(&object, &provider.unchecked_into())
        .unwrap()
        .expect("expected a non-empty line mapping");

    let json = String::from_utf8(bytes).unwrap();
    assert!(
        json.contains("\"__debug-id__\""),
        "missing debug-id sentinel: {json}"
    );
    assert!(json.contains("\"Game.cs\""), "missing C# file: {json}");
    assert!(json.contains("\"2\":42"), "missing line mapping: {json}");

    // Returning a nullish value for every referenced file produces no
    // mapping, so the function returns `None`.
    let empty_provider =
        Closure::<dyn Fn(String) -> JsValue>::new(|_path| JsValue::NULL).into_js_value();
    assert!(
        il2cpp_line_mapping(&object, &empty_provider.unchecked_into())
            .unwrap()
            .is_none()
    );
}
