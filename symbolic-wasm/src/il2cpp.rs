//! Exposes `symbolic_il2cpp`'s line-mapping extraction to WASM.

use symbolic_il2cpp::ObjectLineMapping;
use wasm_bindgen::prelude::*;

use crate::debuginfo::Object;
use crate::utils::{Result, provider_bytes};

/// Extracts a Unity Il2cpp line mapping from `object`, serialized as JSON.
///
/// Unity's Il2cpp transpiles C# to C++, embedding `//<source_info:File.cs:line>`
/// markers in the generated C++. This enumerates the C++ source files referenced
/// by the object, requests each file's contents from `provider`, parses those
/// markers, and returns the C++→C# line mapping as a JSON document (the format
/// Sentry consumes for Il2cpp symbolication).
///
/// `provider` is a `(path: string) => Uint8Array | null | undefined` callback:
/// it receives a referenced source path and returns the file's bytes, or a
/// nullish value to skip it. The callback exists because these bindings have no
/// filesystem access under WebAssembly — the host reads the files and hands back
/// their bytes.
///
/// Returns `undefined` when the object references no Il2cpp `source_info`
/// annotations (i.e. the mapping would be empty).
///
/// This is a free function rather than a method on [`Object`] because it mirrors
/// the standalone `symbolic_il2cpp::ObjectLineMapping`, keeping the Il2cpp-specific
/// concern out of the generic object surface.
#[wasm_bindgen(js_name = il2cppLineMapping)]
pub fn il2cpp_line_mapping(
    object: &Object,
    provider: &js_sys::Function,
) -> Result<Option<Vec<u8>>> {
    let mapping = ObjectLineMapping::from_object_with_provider(object.as_debuginfo(), |path| {
        let value = provider
            .call1(&JsValue::UNDEFINED, &JsValue::from_str(path))
            .unwrap_throw();
        provider_bytes(&value)
    })?;

    let mut buf = Vec::new();
    let written = mapping.to_writer(&mut buf)?;
    Ok(written.then_some(buf))
}
