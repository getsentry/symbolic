use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr;
use std::str::FromStr;

use symbolic::common::byteview::ByteView;
use symbolic::debuginfo::{DebugId, FatObject, Object};

use core::SymbolicStr;

/// A potential multi arch object.
pub struct SymbolicFatObject;

/// A single arch object.
pub struct SymbolicObject;

ffi_fn! {
    /// Loads a fat object from a given path.
    unsafe fn symbolic_fatobject_open(path: *const c_char) -> Result<*mut SymbolicFatObject> {
        let byteview = ByteView::from_path(CStr::from_ptr(path).to_str()?)?;
        let obj = FatObject::parse(byteview)?;
        Ok(Box::into_raw(Box::new(obj)) as *mut SymbolicFatObject)
    }
}

ffi_fn! {
    /// Frees the given fat object.
    unsafe fn symbolic_fatobject_free(sfo: *mut SymbolicFatObject) {
        if !sfo.is_null() {
            let fo = sfo as *mut FatObject<'static>;
            Box::from_raw(fo);
        }
    }
}

ffi_fn! {
    /// Returns the number of contained objects.
    unsafe fn symbolic_fatobject_object_count(sfo: *const SymbolicFatObject) -> Result<usize> {
        let fo = sfo as *const FatObject<'static>;
        Ok((*fo).object_count() as usize)
    }
}

ffi_fn! {
    /// Returns the n-th object.
    unsafe fn symbolic_fatobject_get_object(
        sfo: *const SymbolicFatObject,
        idx: usize,
    ) -> Result<*mut SymbolicObject> {
        let fo = sfo as *const FatObject<'static>;
        if let Some(obj) = (*fo).get_object(idx)? {
            Ok(Box::into_raw(Box::new(obj)) as *mut SymbolicObject)
        } else {
            Ok(ptr::null_mut())
        }
    }
}

ffi_fn! {
    /// Returns the architecture of the object.
    unsafe fn symbolic_object_get_arch(so: *const SymbolicObject) -> Result<SymbolicStr> {
        let o = so as *const Object<'static>;
        Ok(SymbolicStr::new((*o).arch()?.name()))
    }
}

ffi_fn! {
    unsafe fn symbolic_object_get_id(so: *const SymbolicObject) -> Result<SymbolicStr> {
        let o = so as *const Object<'static>;
        Ok((*o).id().unwrap_or_default().to_string().into())
    }
}

ffi_fn! {
    /// Returns the object kind
    unsafe fn symbolic_object_get_kind(so: *const SymbolicObject) -> Result<SymbolicStr> {
        let o = so as *const Object<'static>;
        Ok(SymbolicStr::new((*o).kind().name()))
    }
}

ffi_fn! {
    /// Returns the object type
    unsafe fn symbolic_object_get_type(so: *const SymbolicObject) -> Result<SymbolicStr> {
        let o = so as *mut Object<'static>;
        Ok(SymbolicStr::new((*o).class().name()))
    }
}

ffi_fn! {
    /// Returns the object class
    unsafe fn symbolic_object_get_debug_kind(so: *const SymbolicObject) -> Result<SymbolicStr> {
        let o = so as *const Object<'static>;
        Ok(if let Some(kind) = (*o).debug_kind() {
            SymbolicStr::new(kind.name())
        } else {
            SymbolicStr::default()
        })
    }
}

ffi_fn! {
    /// Frees an object returned from a fat object.
    unsafe fn symbolic_object_free(so: *mut SymbolicObject) {
        if !so.is_null() {
            let o = so as *mut Object<'static>;
            Box::from_raw(o);
        }
    }
}

ffi_fn! {
    /// Converts a Breakpad CodeModuleId to DebugId.
    unsafe fn symbolic_id_from_breakpad(sid: *const SymbolicStr) -> Result<SymbolicStr> {
        Ok(DebugId::from_breakpad((*sid).as_str())?.to_string().into())
    }
}

ffi_fn! {
    /// Normalizes a debug identifier to default representation.
    unsafe fn symbolic_normalize_debug_id(sid: *const SymbolicStr) -> Result<SymbolicStr> {
        Ok(DebugId::from_str((*sid).as_str())?.to_string().into())
    }
}
