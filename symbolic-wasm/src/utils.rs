//! Internal `symbolic-wasm` utilities.
//!
//! This module only contains internal utilities, not utilities exposed to WASM.

use wasm_bindgen::JsError;

/// Helper for creating a [`symbolic_common::SelfCell`] from a value derived from another cell.
///
/// This is unsafe and callers must ensure the derived value only borrows from the data
/// of the owning [`symbolic_common::SelfCell`].
///
/// Two forms are supported:
///
/// - `derived_from_cell!(Ty, owner, derived)` for types re-exported at the
///   `symbolic_debuginfo` crate root (e.g. `Object`, `ObjectDebugSession`).
/// - `derived_from_cell!(Ty<'_>, Ty<'static>, owner, derived)` for types behind a
///   module path (e.g. `pe::PeObject`), where the borrowed and `'static` forms must
///   be spelled out because a `path` fragment cannot be followed by a lifetime.
macro_rules! derived_from_cell {
    ($ty:ident, $owner:expr, $derived:expr) => {
        $crate::utils::derived_from_cell!(
            symbolic_debuginfo::$ty<'_>,
            symbolic_debuginfo::$ty<'static>,
            $owner,
            $derived
        )
    };
    ($borrowed:ty, $static:ty, $owner:expr, $derived:expr) => {{
        let derived = std::mem::transmute::<$borrowed, $static>($derived);
        ::symbolic_common::SelfCell::from_raw($owner.owner().clone(), derived)
    }};
}
pub(crate) use derived_from_cell;

/// An error which can be converted to [`wasm_bindgen::JsValue`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Object(#[from] symbolic_debuginfo::ObjectError),
    #[error(transparent)]
    SourceBundleError(#[from] symbolic_debuginfo::sourcebundle::SourceBundleError),
    #[error(transparent)]
    Pe(#[from] symbolic_debuginfo::pe::PeError),
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
pub fn provider_bytes(value: &wasm_bindgen::JsValue) -> Option<Vec<u8>> {
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
