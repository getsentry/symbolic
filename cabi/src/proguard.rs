use std::ffi::CStr;
use std::os::raw::c_char;
use std::slice;

use symbolic::common::byteview::ByteView;
use symbolic::proguard::ProguardMappingView;

use crate::core::{SymbolicStr, SymbolicUuid};

/// Represents a proguard mapping view.
pub struct SymbolicProguardMappingView;

ffi_fn! {
    /// Creates a proguard mapping view from a path.
    unsafe fn symbolic_proguardmappingview_from_path(
        path: *const c_char
    ) -> Result<*mut SymbolicProguardMappingView> {
        let bv = ByteView::from_path(CStr::from_ptr(path).to_str()?)?;
        let sv = ProguardMappingView::parse(bv)?;
        Ok(Box::into_raw(Box::new(sv)) as *mut SymbolicProguardMappingView)
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
        let s = slice::from_raw_parts(bytes as *const _, len);
        let bv = ByteView::from_slice(s);
        let sv = ProguardMappingView::parse(bv)?;
        Ok(Box::into_raw(Box::new(sv)) as *mut SymbolicProguardMappingView)
    }
}

ffi_fn! {
    /// Frees a proguard mapping view.
    unsafe fn symbolic_proguardmappingview_free(spmv: *mut SymbolicProguardMappingView) {
        if !spmv.is_null() {
            let pmv = spmv as *mut ProguardMappingView<'static>;
            Box::from_raw(pmv);
        }
    }
}

ffi_fn! {
    /// Returns the UUID of a proguard mapping file.
    unsafe fn symbolic_proguardmappingview_get_uuid(
        spmv: *mut SymbolicProguardMappingView,
    ) -> Result<SymbolicUuid> {
        let pmv = spmv as *mut ProguardMappingView<'static>;
        Ok((*pmv).uuid().into())
    }
}

ffi_fn! {
    /// Converts a dotted path at a line number.
    unsafe fn symbolic_proguardmappingview_convert_dotted_path(
        spmv: *const SymbolicProguardMappingView,
        path: *const SymbolicStr,
        lineno: u32,
    ) -> Result<SymbolicStr> {
        let pmv = spmv as *const ProguardMappingView;
        let path = (*path).as_str();
        Ok(SymbolicStr::from_string((*pmv).convert_dotted_path(path, lineno)))
    }
}

ffi_fn! {
    /// Returns true if the mapping file has line infos.
    unsafe fn symbolic_proguardmappingview_has_line_info(
        spmv: *const SymbolicProguardMappingView,
    ) -> Result<bool> {
        let pmv = spmv as *const ProguardMappingView;
        Ok((*pmv).has_line_info())
    }
}
