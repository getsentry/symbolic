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
}

impl From<Error> for wasm_bindgen::JsValue {
    fn from(value: Error) -> Self {
        JsError::new(&value.to_string()).into()
    }
}

/// A common `Result` type for these WASM bindings.
pub type Result<T, E = Error> = std::result::Result<T, E>;
