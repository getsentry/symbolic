use std::mem;
use std::ptr;
use std::os::raw::c_char;
use std::ffi::CStr;

use symbolic_common::ByteView;
use symbolic_debuginfo::{Object, FatObject};

use core::{SymbolicStr, SymbolicUuid};

use uuid::Uuid;

/// A potential multi arch object.
pub struct SymbolicFatObject;

/// A single arch object.
pub struct SymbolicObject;

ffi_fn! {
    /// Loads a fat object from a given path.
    unsafe fn symbolic_fatobject_open(path: *const c_char)
        -> Result<*mut SymbolicFatObject>
    {
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
    unsafe fn symbolic_fatobject_object_count(sfo: *const SymbolicFatObject)
        -> Result<usize>
    {
        let fo = sfo as *const FatObject<'static>;
        Ok((*fo).object_count() as usize)
    }
}

ffi_fn! {
    /// Returns the n-th object.
    unsafe fn symbolic_fatobject_get_object(sfo: *const SymbolicFatObject,
                                            idx: usize)
        -> Result<*mut SymbolicObject>
    {
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
    unsafe fn symbolic_object_get_arch(so: *const SymbolicObject)
        -> Result<SymbolicStr>
    {
        let o = so as *mut Object<'static>;
        Ok(SymbolicStr::new((*o).arch().name()))
    }
}

ffi_fn! {
    /// Returns the UUID of an object.
    unsafe fn symbolic_object_get_uuid(so: *const SymbolicObject)
        -> Result<SymbolicUuid>
    {
        let o = so as *mut Object<'static>;
        Ok(mem::transmute(*(*o).uuid().unwrap_or(Uuid::nil()).as_bytes()))
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
