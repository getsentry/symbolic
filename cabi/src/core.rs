use std::mem;
use std::ptr;
use std::str;
use std::slice;
use std::ffi::CStr;
use std::os::raw::c_char;

use utils::{set_panic_hook, LAST_ERROR, LAST_PANIC};

use uuid::Uuid;

use symbolic_common::ErrorKind;


/// Represents a string.
#[repr(C)]
pub struct SymbolicStr {
    pub data: *mut c_char,
    pub len: usize,
    pub owned: bool,
}

impl Default for SymbolicStr {
    fn default() -> SymbolicStr {
        SymbolicStr {
            data: ptr::null_mut(),
            len: 0,
            owned: false,
        }
    }
}

impl SymbolicStr {
    pub fn new(s: &str) -> SymbolicStr {
        SymbolicStr {
            data: s.as_ptr() as *mut c_char,
            len: s.len(),
            owned: false,
        }
    }

    pub fn from_string(mut s: String) -> SymbolicStr {
        s.shrink_to_fit();
        let rv = SymbolicStr {
            data: s.as_ptr() as *mut c_char,
            len: s.len(),
            owned: true,
        };
        mem::forget(s);
        rv
    }

    pub unsafe fn free(&mut self) {
        if self.owned {
            String::from_raw_parts(self.data as *mut _, self.len, self.len);
            self.data = ptr::null_mut();
            self.len = 0;
            self.owned = false;
        }
    }

    pub fn as_str(&self) -> &str {
        unsafe {
            str::from_utf8_unchecked(slice::from_raw_parts(
                self.data as *const _, self.len))
        }
    }
}

/// Represents a UUID
#[repr(C)]
pub struct SymbolicUuid {
    pub data: [u8; 16]
}

/// Indicates the error that ocurred
#[repr(u32)]
pub enum SymbolicErrorCode {
    NoError = 0,
    Panic = 1,
    Internal = 2,
    Msg = 3,
    Unknown = 4,
    Parse = 101,
    NotFound = 102,
    Format = 103,
    BadSymbol = 1001,
    UnsupportedObjectFile = 1002,
    MalformedObjectFile = 1003,
    BadCacheFile = 1004,
    MissingSection = 1005,
    BadDwarfData = 1006,
    MissingDebugInfo = 1007,
    Io = 10001,
    Utf8Error = 10002,
}

impl SymbolicErrorCode {
    pub fn from_kind(kind: &ErrorKind) -> SymbolicErrorCode {
        match *kind {
            ErrorKind::Panic(..) => SymbolicErrorCode::Panic,
            ErrorKind::Msg(..) => SymbolicErrorCode::Msg,
            ErrorKind::BadSymbol(..) => SymbolicErrorCode::BadSymbol,
            ErrorKind::Internal(..) => SymbolicErrorCode::Internal,
            ErrorKind::Parse(..) => SymbolicErrorCode::Parse,
            ErrorKind::NotFound(..) => SymbolicErrorCode::NotFound,
            ErrorKind::Format(..) => SymbolicErrorCode::Format,
            ErrorKind::UnsupportedObjectFile => SymbolicErrorCode::UnsupportedObjectFile,
            ErrorKind::MalformedObjectFile(..) => SymbolicErrorCode::MalformedObjectFile,
            ErrorKind::BadCacheFile(..) => SymbolicErrorCode::BadCacheFile,
            ErrorKind::MissingSection(..) => SymbolicErrorCode::MissingSection,
            ErrorKind::BadDwarfData(..) => SymbolicErrorCode::BadDwarfData,
            ErrorKind::MissingDebugInfo(..) => SymbolicErrorCode::MissingDebugInfo,
            ErrorKind::Io(..) => SymbolicErrorCode::Io,
            ErrorKind::Utf8Error(..) => SymbolicErrorCode::Utf8Error,
            // we don't use _ here but the hidden field on error kind so that
            // we don't accidentally forget to map them to error codes.
            ErrorKind::__Nonexhaustive { .. } => unreachable!(),
        }
    }
}

/// Initializes the library
#[no_mangle]
pub unsafe extern "C" fn symbolic_init() {
    set_panic_hook();
}

/// Returns the last error code.
///
/// If there is no error, 0 is returned.
#[no_mangle]
pub unsafe extern "C" fn symbolic_err_get_last_code() -> SymbolicErrorCode {
    LAST_ERROR.with(|e| {
        if let Some(ref err) = *e.borrow() {
            SymbolicErrorCode::from_kind(err.kind())
        } else {
            SymbolicErrorCode::NoError
        }
    })
}

/// Returns the last error message.
///
/// If there is no error an empty string is returned.  This allocates new memory
/// that needs to be freed with `symbolic_str_free`.
#[no_mangle]
pub unsafe extern "C" fn symbolic_err_get_last_message() -> SymbolicStr {
    use std::fmt::Write;
    use std::error::Error;
    LAST_ERROR.with(|e| {
        if let Some(ref err) = *e.borrow() {
            let mut msg = err.to_string();
            let mut cause = err.cause();
            while let Some(the_cause) = cause {
                write!(&mut msg, "\n  caused by: {}", the_cause).ok();
                cause = the_cause.cause();
            }
            SymbolicStr::from_string(msg)
        } else {
            Default::default()
        }
    })
}

/// Returns the panic information as string.
#[no_mangle]
pub unsafe extern "C" fn symbolic_err_get_panic_info() -> SymbolicStr {
    LAST_PANIC.with(|e| {
        if let Some((ref info, ref backtrace)) = *e.borrow() {
            use std::fmt::Write;
            let mut out = format!("{}\nstacktrace:", info);
            let frames = backtrace.frames();
            if frames.len() > 5 {
                let mut done = false;
                for frame in frames[6..].iter() {
                    if done {
                        break;
                    }

                    let ip = frame.ip();
                    let symbols = frame.symbols();
                    for symbol in symbols.iter() {
                        write!(&mut out, "\n{:18?} ", ip).ok();

                        if let Some(name) = symbol.name() {
                            write!(&mut out, "{}", name).ok();
                            // hack hack hack: make smaller stacktraces in case we are
                            // a python binding.
                            if name.as_bytes() == b"ffi_call" {
                                done = true;
                            }
                        } else {
                            write!(&mut out, "<unknown>").ok();
                        }

                        if let Some(file) = symbol.filename() {
                            if let Some(filename) = file.file_name() {
                                write!(&mut out, " ({}:{})", filename.to_string_lossy(),
                                       symbol.lineno().unwrap_or(0)).ok();
                            }
                        }
                    }
                }
            }
            SymbolicStr::from_string(out)
        } else {
            Default::default()
        }
    })
}

/// Clears the last error.
#[no_mangle]
pub unsafe extern "C" fn symbolic_err_clear() {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = None;
    });
    LAST_PANIC.with(|e| {
        *e.borrow_mut() = None;
    });
}

ffi_fn! {
    /// Creates a symbolic str from a c string.
    ///
    /// This sets the string to owned.  In case it's not owned you either have
    /// to make sure you are not freeing the memory or you need to set the
    /// owned flag to false.
    unsafe fn symbolic_str_from_cstr(s: *const c_char) -> Result<SymbolicStr> {
        let s = CStr::from_ptr(s).to_str()?;
        Ok(SymbolicStr {
            data: s.as_ptr() as *mut _,
            len: s.len(),
            owned: true,
        })
    }
}

/// Frees a symbolic str.
///
/// If the string is marked as not owned then this function does not
/// do anything.
#[no_mangle]
pub unsafe extern "C" fn symbolic_str_free(s: *mut SymbolicStr) {
    if !s.is_null() {
        (*s).free()
    }
}

/// Returns true if the uuid is nil
#[no_mangle]
pub unsafe extern "C" fn symbolic_uuid_is_nil(uuid: *const SymbolicUuid) -> bool {
    if let Ok(uuid) = Uuid::from_bytes(&(*uuid).data[..]) {
        uuid == Uuid::nil()
    } else {
        false
    }
}

/// Formats the UUID into a string.
///
/// The string is newly allocated and needs to be released with
/// `symbolic_cstr_free`.
#[no_mangle]
pub unsafe extern "C" fn symbolic_uuid_to_str(uuid: *const SymbolicUuid) -> SymbolicStr {
    let uuid =  Uuid::from_bytes(&(*uuid).data[..]).unwrap_or(Uuid::nil());
    SymbolicStr::from_string(uuid.hyphenated().to_string())
}
