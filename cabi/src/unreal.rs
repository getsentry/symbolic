use std::os::raw::c_char;
use std::slice;

use symbolic::common::byteview::ByteView;
use symbolic::minidump::processor::ProcessState;
use symbolic::unreal::{Unreal4Crash, Unreal4CrashFile};

use crate::core::SymbolicStr;
use crate::minidump::SymbolicProcessState;

pub struct SymbolicUnreal4Crash;

pub struct SymbolicUnreal4CrashFile;

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
    unsafe fn symbolic_unreal4_get_context(unreal: *const SymbolicUnreal4Crash) -> Result<SymbolicStr> {
        let unreal = &*(unreal as *const Unreal4Crash);

        let context = unreal.get_context()?;
        Ok(SymbolicStr::from_string(serde_json::to_string(&context)?))
    }
}

ffi_fn! {
    unsafe fn symbolic_unreal4_get_logs(unreal: *const SymbolicUnreal4Crash) -> Result<SymbolicStr> {
        let unreal = &*(unreal as *const Unreal4Crash);

        let context = unreal.get_logs(100)?;
        Ok(SymbolicStr::from_string(serde_json::to_string(&context)?))
    }
}

ffi_fn! {
    unsafe fn symbolic_unreal4_crash_process_minidump(
        unreal: *const SymbolicUnreal4Crash
    ) -> Result<*mut SymbolicProcessState> {
        let unreal = &*(unreal as *const Unreal4Crash);

        let byte_view = match unreal.get_minidump_slice()? {
            Some(bytes) => ByteView::from_slice(bytes),
            None => return Ok(std::ptr::null_mut()),
        };

        let state = ProcessState::from_minidump(&byte_view, None)?;
        let sstate = SymbolicProcessState::from_process_state(&state);
        Ok(Box::into_raw(Box::new(sstate)))
    }
}

ffi_fn! {
    unsafe fn symbolic_unreal4_crash_file_count(unreal: *const SymbolicUnreal4Crash) -> Result<usize> {
        let unreal = unreal as *const Unreal4Crash;
        Ok((*unreal).file_count())
    }
}

ffi_fn! {
    unsafe fn symbolic_unreal4_crash_file_by_index(unreal: *const SymbolicUnreal4Crash, idx: usize) -> Result<*const SymbolicUnreal4CrashFile> {
        let unreal = unreal as *const Unreal4Crash;

        Ok(match (*unreal).file_by_index(idx) {
            Some(f) => f as *const Unreal4CrashFile as *const SymbolicUnreal4CrashFile,
            None => std::ptr::null_mut(),
        })
    }
}

ffi_fn! {
    unsafe fn symbolic_unreal4_crash_file_meta_contents(
            meta: *const SymbolicUnreal4CrashFile,
            unreal: *const SymbolicUnreal4Crash,
            len: *mut usize,
    ) -> Result<*const u8> {
        let unreal = unreal as *const Unreal4Crash;
        let meta = meta as *const Unreal4CrashFile;

        let contents = (*unreal).get_file_contents(&*meta)?;
        if !len.is_null() {
            *len = contents.len();
        }
        Ok(contents.as_ptr())
    }
}

ffi_fn! {
    unsafe fn symbolic_unreal4_crash_file_meta_name(meta: *const SymbolicUnreal4CrashFile) -> Result<SymbolicStr> {
        let meta = meta as *const Unreal4CrashFile;
        Ok((*meta).file_name.clone().into())
    }
}

ffi_fn! {
    unsafe fn symbolic_unreal4_crash_file_meta_type(meta: *const SymbolicUnreal4CrashFile) -> Result<SymbolicStr> {
        let meta = meta as *const Unreal4CrashFile;
        Ok((*meta).ty().name().into())
    }
}
