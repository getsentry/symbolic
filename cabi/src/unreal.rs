use std::os::raw::c_char;
use std::slice;
use std::str::FromStr;

use symbolic::common::ByteView;
use symbolic::minidump::processor::ProcessState;
use symbolic::unreal::{Unreal4Crash, Unreal4CrashFile};

use apple_crash_report_parser::AppleCrashReport;

use crate::core::SymbolicStr;
use crate::minidump::SymbolicProcessState;
use crate::utils::ForeignObject;

/// An Unreal Engine 4 crash report.
pub struct SymbolicUnreal4Crash;

impl ForeignObject for SymbolicUnreal4Crash {
    type RustObject = Unreal4Crash;
}

/// A file contained in a SymbolicUnreal4Crash.
pub struct SymbolicUnreal4CrashFile;

impl ForeignObject for SymbolicUnreal4CrashFile {
    type RustObject = Unreal4CrashFile;
}

ffi_fn! {
    /// Parses an Unreal Engine 4 crash from the given buffer.
    unsafe fn symbolic_unreal4_crash_from_bytes(bytes: *const c_char, len: usize) -> Result<*mut SymbolicUnreal4Crash> {
        let slice = slice::from_raw_parts(bytes as *const _, len);
        let unreal = Unreal4Crash::from_slice(slice)?;
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
        let context = SymbolicUnreal4Crash::as_rust(unreal).get_context()?;
        let json = serde_json::to_string(&context)?;
        Ok(json.into())
    }
}

ffi_fn! {
    /// Parses the Unreal Engine 4 logs from a crash, returns JSON.
    unsafe fn symbolic_unreal4_get_logs(unreal: *const SymbolicUnreal4Crash) -> Result<SymbolicStr> {
        let logs = SymbolicUnreal4Crash::as_rust(unreal).get_logs(100)?;
        let json = serde_json::to_string(&logs)?;
        Ok(json.into())
    }
}

ffi_fn! {
    /// Processes the minidump process state from an Unreal Engine 4 crash.
    unsafe fn symbolic_unreal4_crash_process_minidump(
        unreal: *const SymbolicUnreal4Crash
    ) -> Result<*mut SymbolicProcessState> {
        let byte_view = match SymbolicUnreal4Crash::as_rust(unreal).get_minidump_slice()? {
            Some(bytes) => ByteView::from_slice(bytes),
            None => return Ok(std::ptr::null_mut()),
        };

        let state = ProcessState::from_minidump(&byte_view, None)?;
        let sstate = SymbolicProcessState::from_process_state(&state);
        Ok(Box::into_raw(Box::new(sstate)))
    }
}

ffi_fn! {
    /// Parses the Apple crash report from an Unreal Engine 4 crash.
    unsafe fn symbolic_unreal4_crash_get_apple_crash_report(
        unreal: *const SymbolicUnreal4Crash
    ) -> Result<SymbolicStr> {
        Ok(match SymbolicUnreal4Crash::as_rust(unreal).get_apple_crash_report()? {
            Some(report) => {
                let report = AppleCrashReport::from_str(report)?;
                serde_json::to_string(&report)?.into()
            }
            None => "{}".into()
        })
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
    ) -> Result<*const SymbolicUnreal4CrashFile> {
        Ok(match SymbolicUnreal4Crash::as_rust(unreal).file_by_index(index) {
            Some(file) => SymbolicUnreal4CrashFile::from_ref(file),
            None => std::ptr::null()
        })
    }
}

ffi_fn! {
    /// Returns file contents data of a file in the Unreal 4 crash.
    unsafe fn symbolic_unreal4_crash_file_contents(
        file: *const SymbolicUnreal4CrashFile,
        unreal: *const SymbolicUnreal4Crash,
        len: *mut usize,
    ) -> Result<*const u8> {
        let file = SymbolicUnreal4CrashFile::as_rust(file);
        let contents = SymbolicUnreal4Crash::as_rust(unreal).get_file_contents(file)?;

        if !len.is_null() {
            *len = contents.len();
        }

        Ok(contents.as_ptr())
    }
}

ffi_fn! {
    /// Returns the file name of a file in the Unreal 4 crash.
    unsafe fn symbolic_unreal4_crash_file_name(
        file: *const SymbolicUnreal4CrashFile
    ) -> Result<SymbolicStr> {
        Ok(SymbolicUnreal4CrashFile::as_rust(file).file_name.as_str().into())
    }
}

ffi_fn! {
    /// Returns the file type of a file in the Unreal 4 crash.
    unsafe fn symbolic_unreal4_crash_file_type(
        file: *const SymbolicUnreal4CrashFile
    ) -> Result<SymbolicStr> {
        Ok(SymbolicUnreal4CrashFile::as_rust(file).ty().name().into())
    }
}
