use std::os::raw::c_char;
use std::slice;

use symbolic::unreal::{Unreal4Crash, Unreal4File};

use crate::core::SymbolicStr;
use crate::utils::ForeignObject;

/// An Unreal Engine 4 crash report.
pub struct SymbolicUnreal4Crash;

impl ForeignObject for SymbolicUnreal4Crash {
    type RustObject = Unreal4Crash;
}

/// A file contained in a SymbolicUnreal4Crash.
pub struct SymbolicUnreal4File;

impl ForeignObject for SymbolicUnreal4File {
    type RustObject = Unreal4File;
}

ffi_fn! {
    /// Parses an Unreal Engine 4 crash from the given buffer.
    unsafe fn symbolic_unreal4_crash_from_bytes(bytes: *const c_char, len: usize) -> Result<*mut SymbolicUnreal4Crash> {
        let slice = slice::from_raw_parts(bytes as *const _, len);
        let unreal = Unreal4Crash::parse(slice)?;
        Ok(SymbolicUnreal4Crash::from_rust(unreal))
    }
}

ffi_fn! {
    /// Frees an Unreal Engine 4 crash.
    unsafe fn symbolic_unreal4_crash_free(unreal: *mut SymbolicUnreal4Crash) {
        SymbolicUnreal4Crash::drop(unreal)
    }
}

ffi_fn! {
    /// Parses the Unreal Engine 4 context from a crash, returns JSON.
    unsafe fn symbolic_unreal4_get_context(
        unreal: *const SymbolicUnreal4Crash
    ) -> Result<SymbolicStr> {
        let context = SymbolicUnreal4Crash::as_rust(unreal).context()?;
        let json = serde_json::to_string(&context)?;
        Ok(json.into())
    }
}

ffi_fn! {
    /// Parses the Unreal Engine 4 logs from a crash, returns JSON.
    unsafe fn symbolic_unreal4_get_logs(unreal: *const SymbolicUnreal4Crash) -> Result<SymbolicStr> {
        let logs = SymbolicUnreal4Crash::as_rust(unreal).logs(100)?;
        let json = serde_json::to_string(&logs)?;
        Ok(json.into())
    }
}

ffi_fn! {
    /// Returns the number of files in the Unreal 4 crash.
    unsafe fn symbolic_unreal4_crash_file_count(
        unreal: *const SymbolicUnreal4Crash
    ) -> Result<usize> {
        Ok(SymbolicUnreal4Crash::as_rust(unreal).file_count())
    }
}

ffi_fn! {
    /// Returns file meta data of a file in the Unreal 4 crash.
    unsafe fn symbolic_unreal4_crash_file_by_index(
        unreal: *const SymbolicUnreal4Crash,
        index: usize,
    ) -> Result<*mut SymbolicUnreal4File> {
        Ok(match SymbolicUnreal4Crash::as_rust(unreal).file_by_index(index) {
            Some(file) => SymbolicUnreal4File::from_rust(file),
            None => std::ptr::null_mut()
        })
    }
}

ffi_fn! {
    /// Returns the file name of a file in the Unreal 4 crash.
    unsafe fn symbolic_unreal4_file_name(
        file: *const SymbolicUnreal4File
    ) -> Result<SymbolicStr> {
        Ok(SymbolicUnreal4File::as_rust(file).name().into())
    }
}

ffi_fn! {
    /// Returns the file type of a file in the Unreal 4 crash.
    unsafe fn symbolic_unreal4_file_type(
        file: *const SymbolicUnreal4File
    ) -> Result<SymbolicStr> {
        Ok(SymbolicUnreal4File::as_rust(file).ty().name().into())
    }
}

ffi_fn! {
    /// Returns the file contents of a file in the Unreal 4 crash.
    unsafe fn symbolic_unreal4_file_data(
        file: *const SymbolicUnreal4File,
        len: *mut usize,
    ) -> Result<*const u8> {
        let data = SymbolicUnreal4File::as_rust(file).data();

        if !len.is_null() {
            *len = data.len();
        }

        Ok(data.as_ptr())
    }
}

ffi_fn! {
    /// Frees an Unreal Engine 4 file.
    unsafe fn symbolic_unreal4_file_free(file: *mut SymbolicUnreal4File) {
        SymbolicUnreal4File::drop(file)
    }
}
