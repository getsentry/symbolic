use std::os::raw::c_char;
use std::slice;

use symbolic::unreal::Unreal4Crash;

pub struct SymbolicUnreal4Crash;

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
