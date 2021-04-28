use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr;
use std::slice;
use std::str::FromStr;

use symbolic::common::{ByteView, CodeId, DebugId, SelfCell};
use symbolic::debuginfo::macho::{BcSymbolMap, UuidMapping};
use symbolic::debuginfo::{Archive, Object};

use crate::core::SymbolicStr;
use crate::utils::ForeignObject;

/// A potential multi arch object.
pub struct SymbolicArchive;

impl ForeignObject for SymbolicArchive {
    type RustObject = SelfCell<ByteView<'static>, Archive<'static>>;
}

/// A single arch object.
pub struct SymbolicObject;

impl ForeignObject for SymbolicObject {
    type RustObject = SelfCell<ByteView<'static>, Object<'static>>;
}

/// Features this object contains.
#[repr(C)]
pub struct SymbolicObjectFeatures {
    symtab: bool,
    debug: bool,
    unwind: bool,
    sources: bool,
}

/// A `BCSymbolMap`.
pub struct SymbolicBcSymbolMap;

impl ForeignObject for SymbolicBcSymbolMap {
    type RustObject = SelfCell<ByteView<'static>, BcSymbolMap<'static>>;
}

/// A UUID mapping.
pub struct SymbolicUuidMapping;

impl ForeignObject for SymbolicUuidMapping {
    type RustObject = UuidMapping;
}

ffi_fn! {
    /// Loads an archive from a given path.
    unsafe fn symbolic_archive_open(path: *const c_char) -> Result<*mut SymbolicArchive> {
        let byteview = ByteView::open(CStr::from_ptr(path).to_str()?)?;
        let cell = SelfCell::try_new(byteview, |p| Archive::parse(&*p))?;
        Ok(SymbolicArchive::from_rust(cell))
    }
}

ffi_fn! {
    /// Creates an archive from a byte buffer without taking ownership of the pointer.
    unsafe fn symbolic_archive_from_bytes(
        bytes: *const u8,
        len: usize,
    ) -> Result<*mut SymbolicArchive> {
        let byteview = ByteView::from_slice(slice::from_raw_parts(bytes, len));
        let cell = SelfCell::try_new(byteview, |p| Archive::parse(&*p))?;
        Ok(SymbolicArchive::from_rust(cell))
    }
}

ffi_fn! {
    /// Frees the given archive.
    unsafe fn symbolic_archive_free(archive: *mut SymbolicArchive) {
        SymbolicArchive::drop(archive)
    }
}

ffi_fn! {
    /// Returns the number of contained objects.
    unsafe fn symbolic_archive_object_count(archive: *const SymbolicArchive) -> Result<usize> {
        Ok(SymbolicArchive::as_rust(archive).get().object_count())
    }
}

ffi_fn! {
    /// Returns the n-th object, or a null pointer if the object does not exist.
    unsafe fn symbolic_archive_get_object(
        archive: *const SymbolicArchive,
        index: usize,
    ) -> Result<*mut SymbolicObject> {
        let archive = SymbolicArchive::as_rust(archive);
        if let Some(object) = archive.get().object_by_index(index)? {
            let object = SelfCell::from_raw(archive.owner().clone(), object);
            Ok(SymbolicObject::from_rust(object))
        } else {
            Ok(ptr::null_mut())
        }
    }
}

ffi_fn! {
    /// Returns the architecture of the object.
    unsafe fn symbolic_object_get_arch(object: *const SymbolicObject) -> Result<SymbolicStr> {
        Ok(SymbolicObject::as_rust(object).get().arch().name().into())
    }
}

ffi_fn! {
    /// Returns the code identifier of the object.
    unsafe fn symbolic_object_get_code_id(object: *const SymbolicObject) -> Result<SymbolicStr> {
        let id_opt = SymbolicObject::as_rust(object).get().code_id();
        Ok(id_opt.unwrap_or_default().to_string().into())
    }
}

ffi_fn! {
    /// Returns the debug identifier of the object.
    unsafe fn symbolic_object_get_debug_id(object: *const SymbolicObject) -> Result<SymbolicStr> {
        Ok(SymbolicObject::as_rust(object).get().debug_id().to_string().into())
    }
}

ffi_fn! {
    /// Returns the object kind (e.g. executable, debug file, library, ...).
    unsafe fn symbolic_object_get_kind(object: *const SymbolicObject) -> Result<SymbolicStr> {
        Ok(SymbolicObject::as_rust(object).get().kind().name().into())
    }
}

ffi_fn! {
    /// Returns the file format of the object file (e.g. MachO, ELF, ...).
    unsafe fn symbolic_object_get_file_format(
        object: *const SymbolicObject
    ) -> Result<SymbolicStr> {
        Ok(SymbolicObject::as_rust(object).get().file_format().name().into())
    }
}

ffi_fn! {
    unsafe fn symbolic_object_get_features(
        object: *const SymbolicObject,
    ) -> Result<SymbolicObjectFeatures> {
        let object = SymbolicObject::as_rust(object).get();
        Ok(SymbolicObjectFeatures {
            symtab: object.has_symbols(),
            debug: object.has_debug_info(),
            unwind: object.has_unwind_info(),
            sources: object.has_sources(),
        })
    }
}

ffi_fn! {
    /// Frees an object returned from an archive.
    unsafe fn symbolic_object_free(object: *mut SymbolicObject) {
        SymbolicObject::drop(object);
    }
}

ffi_fn! {
    /// Loads a BCSymbolmap from a given path
    unsafe fn symbolic_bcsymbolmap_open(path: *const c_char,) -> Result<*mut SymbolicBcSymbolMap> {
        let byteview = ByteView::open(CStr::from_ptr(path).to_str()?)?;
        let cell = SelfCell::try_new(byteview, |ptr| BcSymbolMap::parse(&*ptr))?;
        Ok(SymbolicBcSymbolMap::from_rust(cell))
    }
}

ffi_fn! {
    /// Frees the given BcSymbolMap.
    unsafe fn symbolic_bcsymbolmap_free(bcsymbolmap: *mut SymbolicBcSymbolMap) {
        SymbolicBcSymbolMap::drop(bcsymbolmap)
    }
}

ffi_fn! {
    /// Loads a UuidMapping by parsing the PList at `path`.
    unsafe fn symbolic_uuidmapping_from_plist(
        debug_id: *const SymbolicStr,
        path: *const c_char,
    ) -> Result<*mut SymbolicUuidMapping> {
        let debug_id = DebugId::from_str((*debug_id).as_str())?;
        let byteview = ByteView::open(CStr::from_ptr(path).to_str()?)?;
        let mapping = UuidMapping::parse_plist(debug_id, &byteview)?;
        Ok(SymbolicUuidMapping::from_rust(mapping))
    }
}

ffi_fn! {
    /// Frees the given UuidMapping.
    unsafe fn symbolic_uuidmapping_free(mapping: *mut SymbolicUuidMapping) {
        SymbolicUuidMapping::drop(mapping)
    }
}

ffi_fn! {
    /// Converts a Breakpad CodeModuleId to DebugId.
    unsafe fn symbolic_id_from_breakpad(breakpad_id: *const SymbolicStr) -> Result<SymbolicStr> {
        Ok(DebugId::from_breakpad((*breakpad_id).as_str())?.to_string().into())
    }
}

ffi_fn! {
    /// Normalizes a code identifier to default representation.
    unsafe fn symbolic_normalize_code_id(code_id: *const SymbolicStr) -> Result<SymbolicStr> {
        Ok(CodeId::from_str((*code_id).as_str())?.to_string().into())
    }
}

ffi_fn! {
    /// Normalizes a debug identifier to default representation.
    unsafe fn symbolic_normalize_debug_id(debug_id: *const SymbolicStr) -> Result<SymbolicStr> {
        Ok(DebugId::from_str((*debug_id).as_str())?.to_string().into())
    }
}
