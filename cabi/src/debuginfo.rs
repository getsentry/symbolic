use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr;
use std::str::FromStr;

use symbolic::common::{ByteView, DebugId, SelfCell};
use symbolic::debuginfo::{Archive, Object};

use crate::core::SymbolicStr;

/// Helper to keep a `ByteView` open with an `Archive` referencing it.
pub(crate) type ArchiveCell = SelfCell<ByteView<'static>, Archive<'static>>;
/// Helper to keep a `ByteView` open with an `Object` referencing it.
pub(crate) type ObjectCell = SelfCell<ByteView<'static>, Object<'static>>;

/// A potential multi arch object.
pub struct SymbolicFatObject;

/// A single arch object.
pub struct SymbolicObject;

/// Features this object contains.
#[repr(C)]
pub struct SymbolicObjectFeatures {
    symtab: bool,
    debug: bool,
    unwind: bool,
}

ffi_fn! {
    /// Loads a fat object from a given path.
    unsafe fn symbolic_fatobject_open(path: *const c_char) -> Result<*mut SymbolicFatObject> {
        let byteview = ByteView::open(CStr::from_ptr(path).to_str()?)?;
        let cell = ArchiveCell::try_new(byteview, |p| Archive::parse(&*p))?;
        Ok(Box::into_raw(Box::new(cell)) as *mut SymbolicFatObject)
    }
}

ffi_fn! {
    /// Frees the given fat object.
    unsafe fn symbolic_fatobject_free(sfo: *mut SymbolicFatObject) {
        if !sfo.is_null() {
            let fo = sfo as *mut ArchiveCell;
            Box::from_raw(fo);
        }
    }
}

ffi_fn! {
    /// Returns the number of contained objects.
    unsafe fn symbolic_fatobject_object_count(sfo: *const SymbolicFatObject) -> Result<usize> {
        let fo = sfo as *const ArchiveCell;
        Ok((*fo).get().object_count() as usize)
    }
}

ffi_fn! {
    /// Returns the n-th object.
    unsafe fn symbolic_fatobject_get_object(
        sfo: *const SymbolicFatObject,
        idx: usize,
    ) -> Result<*mut SymbolicObject> {
        let fo = sfo as *const ArchiveCell;
        if let Some(obj) = (*fo).get().object_by_index(idx)? {
            Ok(Box::into_raw(Box::new(obj)) as *mut SymbolicObject)
        } else {
            Ok(ptr::null_mut())
        }
    }
}

ffi_fn! {
    /// Returns the architecture of the object.
    unsafe fn symbolic_object_get_arch(so: *const SymbolicObject) -> Result<SymbolicStr> {
        let o = so as *const ObjectCell;
        Ok(SymbolicStr::new((*o).get().arch().name()))
    }
}

ffi_fn! {
    /// Returns the debug identifier of the object.
    unsafe fn symbolic_object_get_debug_id(so: *const SymbolicObject) -> Result<SymbolicStr> {
        let o = so as *const ObjectCell;
        Ok((*o).get().debug_id().to_string().into())
    }
}

ffi_fn! {
    /// Returns the object kind (e.g. executable, debug file, library, ...).
    unsafe fn symbolic_object_get_kind(so: *const SymbolicObject) -> Result<SymbolicStr> {
        let o = so as *const ObjectCell;
        Ok(SymbolicStr::new((*o).get().kind().name()))
    }
}

ffi_fn! {
    /// Returns the file format of the object file (e.g. MachO, ELF, ...).
    unsafe fn symbolic_object_get_file_format(so: *const SymbolicObject) -> Result<SymbolicStr> {
        let o = so as *mut ObjectCell;
        Ok(SymbolicStr::new((*o).get().file_format().name()))
    }
}

ffi_fn! {
    unsafe fn symbolic_object_get_features(
        so: *const SymbolicObject,
    ) -> Result<SymbolicObjectFeatures> {
        let o = so as *const ObjectCell;
        let object = (*o).get();

        Ok(SymbolicObjectFeatures {
            symtab: object.has_symbols(),
            debug: object.has_debug_info(),
            unwind: object.has_unwind_info(),
        })
    }
}

ffi_fn! {
    /// Frees an object returned from a fat object.
    unsafe fn symbolic_object_free(so: *mut SymbolicObject) {
        if !so.is_null() {
            let o = so as *mut ObjectCell;
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
