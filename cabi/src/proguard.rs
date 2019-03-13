use std::ffi::CStr;
use std::os::raw::c_char;
use std::slice;

use symbolic::common::ByteView;
use symbolic::proguard::ProguardMappingView;

use crate::core::{SymbolicStr, SymbolicUuid};
use crate::utils::ForeignObject;

/// Represents a proguard mapping view.
pub struct SymbolicProguardMappingView;

impl ForeignObject for SymbolicProguardMappingView {
    type RustObject = ProguardMappingView<'static>;
}

ffi_fn! {
    /// Creates a proguard mapping view from a path.
    unsafe fn symbolic_proguardmappingview_open(
        path: *const c_char
    ) -> Result<*mut SymbolicProguardMappingView> {
        let byteview = ByteView::open(CStr::from_ptr(path).to_str()?)?;
        let proguard = ProguardMappingView::parse(byteview)?;
        Ok(SymbolicProguardMappingView::from_rust(proguard))
    }
}

ffi_fn! {
    /// Creates a proguard mapping view from bytes.
    ///
    /// This shares the underlying memory and does not copy it.
    unsafe fn symbolic_proguardmappingview_from_bytes(
        bytes: *const c_char,
        len: usize
    ) -> Result<*mut SymbolicProguardMappingView> {
        let slice = slice::from_raw_parts(bytes as *const _, len);
        let byteview = ByteView::from_slice(slice);
        let proguard = ProguardMappingView::parse(byteview)?;
        Ok(SymbolicProguardMappingView::from_rust(proguard))
    }
}

ffi_fn! {
    /// Frees a proguard mapping view.
    unsafe fn symbolic_proguardmappingview_free(view: *mut SymbolicProguardMappingView) {
        SymbolicProguardMappingView::drop(view);
    }
}

ffi_fn! {
    /// Returns the UUID of a proguard mapping file.
    unsafe fn symbolic_proguardmappingview_get_uuid(
        view: *mut SymbolicProguardMappingView,
    ) -> Result<SymbolicUuid> {
        Ok(SymbolicProguardMappingView::as_rust(view).uuid().into())
    }
}

ffi_fn! {
    /// Converts a dotted path at a line number.
    unsafe fn symbolic_proguardmappingview_convert_dotted_path(
        view: *const SymbolicProguardMappingView,
        path: *const SymbolicStr,
        lineno: u32,
    ) -> Result<SymbolicStr> {
        let path = (*path).as_str();
        Ok(SymbolicProguardMappingView::as_rust(view).convert_dotted_path(path, lineno).into())
    }
}

ffi_fn! {
    /// Returns true if the mapping file has line infos.
    unsafe fn symbolic_proguardmappingview_has_line_info(
        view: *const SymbolicProguardMappingView,
    ) -> Result<bool> {
        Ok(SymbolicProguardMappingView::as_rust(view).has_line_info())
    }
}
