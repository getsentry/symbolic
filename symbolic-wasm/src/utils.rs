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
    SourceBundle(#[from] symbolic_debuginfo::sourcebundle::SourceBundleError),
    #[error(transparent)]
    Pe(#[from] symbolic_debuginfo::pe::PeError),
}

impl From<Error> for wasm_bindgen::JsValue {
    fn from(value: Error) -> Self {
        JsError::new(&value.to_string()).into()
    }
}

/// A common `Result` type for these WASM bindings.
pub type Result<T, E = Error> = std::result::Result<T, E>;
