//! Internal `symbolic-wasm` utilities.
//!
//! This module only contains internal utilities, not utilities exposed to WASM.

use wasm_bindgen::JsError;

/// Helper for creating a [`symbolic_common::SelfCell`] from a value derived from another cell.
///
/// This is unsafe and callers must ensure the derived value only borrows from the data
/// of the owning [`symbolic_common::SelfCell`].
macro_rules! derived_from_cell {
    ($ty:ident, $owner:expr, $derived:expr) => {{
        let derived = std::mem::transmute::<
            // Temporary workaround, once we expand the functionality of the crate,
            // we'll have to fix the macro to accepts `path::type`.
            symbolic_debuginfo::$ty<'_>,
            symbolic_debuginfo::$ty<'static>,
        >($derived);
        ::symbolic_common::SelfCell::from_raw($owner.owner().clone(), derived)
    }};
}
pub(crate) use derived_from_cell;

/// An error which can be converted to [`wasm_bindgen::JsValue`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    ObjectError(#[from] symbolic_debuginfo::ObjectError),
    #[error(transparent)]
    SourceBundleError(#[from] symbolic_debuginfo::sourcebundle::SourceBundleError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<Error> for wasm_bindgen::JsValue {
    fn from(value: Error) -> Self {
        JsError::new(&value.to_string()).into()
    }
}

/// A common `Result` type for these WASM bindings.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Converts a provider callback's return value into raw bytes.
///
/// Provider callbacks in these bindings are documented as
/// `(path) => Uint8Array | null | undefined`. A nullish value is the documented
/// "skip this file" signal and yields `None`; a `Uint8Array` yields its bytes.
///
/// Any other type throws a descriptive JS error rather than silently mis-coercing
/// it: `js_sys::Uint8Array::new` would turn a `number` into a zero-filled buffer
/// (silent data corruption) and a plain object into an empty one, while other
/// values raise an opaque `TypeError`. Validating here keeps a misbehaving callback
/// from corrupting output or aborting the module with an unhelpful panic.
pub(crate) fn provider_bytes(value: &wasm_bindgen::JsValue) -> Option<Vec<u8>> {
    use wasm_bindgen::JsCast;

    if value.is_null_or_undefined() {
        return None;
    }

    match value.dyn_ref::<js_sys::Uint8Array>() {
        Some(array) => Some(array.to_vec()),
        None => wasm_bindgen::throw_val(
            JsError::new("provider callback must return a Uint8Array, null, or undefined").into(),
        ),
    }
}
