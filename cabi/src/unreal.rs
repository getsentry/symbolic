use std::os::raw::c_char;
use std::slice;

use symbolic::unreal::{CrashFileMeta, Unreal4Crash};

use crate::core::SymbolicStr;

pub struct SymbolicUnreal4Crash;

pub struct SymbolicCrashFileMeta;

ffi_fn! {
    unsafe fn symbolic_unreal4_crash_from_bytes(bytes: *const c_char, len: usize) -> Result<*mut SymbolicUnreal4Crash> {
        let unreal = Unreal4Crash::from_slice(slice::from_raw_parts(bytes as *const _, len))?;
        Ok(Box::into_raw(Box::new(unreal)) as *mut SymbolicUnreal4Crash)
    }
}

ffi_fn! {
    unsafe fn symbolic_unreal4_crash_free(unreal: *mut SymbolicUnreal4Crash) {
        if !unreal.is_null() {
            let unreal = unreal as *mut Unreal4Crash;
            Box::from_raw(unreal);
        }
    }
}

ffi_fn! {
    unsafe fn symbolic_unreal4_crash_get_minidump_bytes(unreal: *const SymbolicUnreal4Crash, len: *mut usize) -> Result<*const u8> {
        let unreal = unreal as *const Unreal4Crash;
        Ok(match (*unreal).get_minidump_bytes()? {
            Some(bytes) => {
                if !len.is_null() {
                    *len = bytes.len();
                }
                bytes.as_ptr()
            }
            None => std::ptr::null(),
        })
    }
}

ffi_fn! {
    unsafe fn symbolic_unreal4_crash_file_count(unreal: *const SymbolicUnreal4Crash) -> Result<usize> {
        let unreal = unreal as *const Unreal4Crash;
        Ok((*unreal).file_count())
    }
}

ffi_fn! {
    unsafe fn symbolic_unreal4_crash_file_by_index(unreal: *const SymbolicUnreal4Crash, idx: usize) -> Result<*const SymbolicCrashFileMeta> {
        let unreal = unreal as *const Unreal4Crash;

        Ok(match (*unreal).file_by_index(idx) {
            Some(f) => f as *const CrashFileMeta as *const SymbolicCrashFileMeta,
            None => std::ptr::null_mut(),
        })
    }
}

ffi_fn! {
    unsafe fn symbolic_unreal4_crash_file_meta_contents(
            meta: *const SymbolicCrashFileMeta,
            unreal: *const SymbolicUnreal4Crash,
            len: *mut usize,
    ) -> Result<*const u8> {
        let unreal = unreal as *const Unreal4Crash;
        let meta = meta as *const CrashFileMeta;

        let contents = (*unreal).get_file_contents(&*meta)?;
        if !len.is_null() {
            *len = contents.len();
        }
        Ok(contents.as_ptr())
    }
}

ffi_fn! {
    unsafe fn symbolic_unreal4_crash_file_meta_name(meta: *const SymbolicCrashFileMeta) -> Result<SymbolicStr> {
        let meta = meta as *const CrashFileMeta;
        Ok((*meta).file_name.clone().into())
    }
}
