use std::mem;
use std::ptr;
use std::str;
use std::slice;
use std::ffi::CStr;
use std::os::raw::c_char;

use uuid::Uuid;
use failure::Error;

use utils::{set_panic_hook, Panic, LAST_ERROR};

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
        unsafe { str::from_utf8_unchecked(slice::from_raw_parts(self.data as *const _, self.len)) }
    }
}

impl From<String> for SymbolicStr {
    fn from(string: String) -> SymbolicStr {
        SymbolicStr::from_string(string)
    }
}

impl<'a> From<&'a str> for SymbolicStr {
    fn from(string: &str) -> SymbolicStr {
        SymbolicStr::new(string)
    }
}

/// Represents a UUID.
#[repr(C)]
pub struct SymbolicUuid {
    pub data: [u8; 16],
}

impl SymbolicUuid {
    pub fn new(uuid: Uuid) -> SymbolicUuid {
        unsafe { mem::transmute(*uuid.as_bytes()) }
    }

    pub fn as_uuid(&self) -> &Uuid {
        unsafe { mem::transmute(self) }
    }
}

impl Default for SymbolicUuid {
    fn default() -> SymbolicUuid {
        Uuid::nil().into()
    }
}

impl From<Uuid> for SymbolicUuid {
    fn from(uuid: Uuid) -> SymbolicUuid {
        SymbolicUuid::new(uuid)
    }
}

/// Represents all possible error codes.
#[repr(u32)]
pub enum SymbolicErrorCode {
    NoError = 0,
    Panic = 1,
    Unknown = 2,

    // std::io
    IoError = 101,

    // symbolic::common::types
    UnknownArchError = 1001,
    UnknownLanguageError = 1002,
    UnknownObjectKindError = 1003,
    UnknownObjectClassError = 1004,
    UnknownDebugKindError = 1005,

    // symbolic::debuginfo
    ParseBreakpadError = 2001,
    ParseDebugIdError = 2002,
    ObjectErrorUnsupportedObject = 2003,
    ObjectErrorBadObject = 2004,
    ObjectErrorMissingSymbolTable = 2005,
    ObjectErrorUnsupportedSymbolTable = 2006,

    // symbolic::minidump::cfi
    CfiErrorMissingDebugInfo = 3001,
    CfiErrorUnsupportedDebugFormat = 3002,
    CfiErrorBadDebugInfo = 3003,
    CfiErrorUnsupportedArch = 3004,
    CfiErrorWriteError = 3005,

    // symbolic::minidump::processor
    ProcessMinidumpError = 4001,

    // symbolic::sourcemap
    ParseSourceMapError = 5001,

    // symbolic::symcache
    SymCacheErrorBadFileMagic = 6001,
    SymCacheErrorBadFileHeader = 6002,
    SymCacheErrorBadSegment = 6003,
    SymCacheErrorBadCacheFile = 6004,
    SymCacheErrorUnsupportedVersion = 6005,
    SymCacheErrorBadDebugFile = 6006,
    SymCacheErrorMissingDebugSection = 6007,
    SymCacheErrorMissingDebugInfo = 6008,
    SymCacheErrorUnsupportedDebugKind = 6009,
    SymCacheErrorValueTooLarge = 6010,
    SymCacheErrorWriteFailed = 6011,
}

impl SymbolicErrorCode {
    /// This maps all errors that can possibly happen.
    pub fn from_error(error: &Error) -> SymbolicErrorCode {
        for cause in error.causes() {
            if let Some(_) = cause.downcast_ref::<Panic>() {
                return SymbolicErrorCode::Panic;
            }

            use std::io::Error as IoError;
            if let Some(_) = cause.downcast_ref::<IoError>() {
                return SymbolicErrorCode::IoError;
            }

            use symbolic::common::types::{UnknownArchError, UnknownDebugKindError,
                                          UnknownLanguageError, UnknownObjectClassError,
                                          UnknownObjectKindError};
            if let Some(_) = cause.downcast_ref::<UnknownArchError>() {
                return SymbolicErrorCode::UnknownArchError;
            } else if let Some(_) = cause.downcast_ref::<UnknownLanguageError>() {
                return SymbolicErrorCode::UnknownLanguageError;
            } else if let Some(_) = cause.downcast_ref::<UnknownDebugKindError>() {
                return SymbolicErrorCode::UnknownDebugKindError;
            } else if let Some(_) = cause.downcast_ref::<UnknownObjectClassError>() {
                return SymbolicErrorCode::UnknownObjectClassError;
            } else if let Some(_) = cause.downcast_ref::<UnknownObjectKindError>() {
                return SymbolicErrorCode::UnknownObjectKindError;
            }

            use symbolic::debuginfo::{ObjectError, ObjectErrorKind, ParseBreakpadError,
                                      ParseDebugIdError};
            if let Some(_) = cause.downcast_ref::<ParseBreakpadError>() {
                return SymbolicErrorCode::ParseBreakpadError;
            } else if let Some(_) = cause.downcast_ref::<ParseDebugIdError>() {
                return SymbolicErrorCode::ParseDebugIdError;
            } else if let Some(error) = cause.downcast_ref::<ObjectError>() {
                return match error.kind() {
                    ObjectErrorKind::UnsupportedObject => {
                        SymbolicErrorCode::ObjectErrorUnsupportedObject
                    }
                    ObjectErrorKind::BadObject => SymbolicErrorCode::ObjectErrorBadObject,
                    ObjectErrorKind::MissingSymbolTable => {
                        SymbolicErrorCode::ObjectErrorMissingSymbolTable
                    }
                    ObjectErrorKind::UnsupportedSymbolTable => {
                        SymbolicErrorCode::ObjectErrorUnsupportedSymbolTable
                    }
                };
            }

            use symbolic::minidump::cfi::{CfiError, CfiErrorKind};
            if let Some(error) = cause.downcast_ref::<CfiError>() {
                return match error.kind() {
                    CfiErrorKind::MissingDebugInfo => SymbolicErrorCode::CfiErrorMissingDebugInfo,
                    CfiErrorKind::UnsupportedDebugFormat => {
                        SymbolicErrorCode::CfiErrorUnsupportedDebugFormat
                    }
                    CfiErrorKind::BadDebugInfo => SymbolicErrorCode::CfiErrorBadDebugInfo,
                    CfiErrorKind::UnsupportedArch => SymbolicErrorCode::CfiErrorUnsupportedArch,
                    CfiErrorKind::WriteError => SymbolicErrorCode::CfiErrorWriteError,
                };
            }

            use symbolic::minidump::processor::ProcessMinidumpError;
            if let Some(_) = cause.downcast_ref::<ProcessMinidumpError>() {
                return SymbolicErrorCode::ProcessMinidumpError;
            }

            use symbolic::sourcemap::ParseSourceMapError;
            if let Some(_) = cause.downcast_ref::<ParseSourceMapError>() {
                return SymbolicErrorCode::ParseSourceMapError;
            }

            use symbolic::symcache::{SymCacheError, SymCacheErrorKind};
            if let Some(error) = cause.downcast_ref::<SymCacheError>() {
                return match error.kind() {
                    SymCacheErrorKind::BadFileMagic => SymbolicErrorCode::SymCacheErrorBadFileMagic,
                    SymCacheErrorKind::BadFileHeader => {
                        SymbolicErrorCode::SymCacheErrorBadFileHeader
                    }
                    SymCacheErrorKind::BadSegment => SymbolicErrorCode::SymCacheErrorBadSegment,
                    SymCacheErrorKind::BadCacheFile => SymbolicErrorCode::SymCacheErrorBadCacheFile,
                    SymCacheErrorKind::UnsupportedVersion => {
                        SymbolicErrorCode::SymCacheErrorUnsupportedVersion
                    }
                    SymCacheErrorKind::BadDebugFile => SymbolicErrorCode::SymCacheErrorBadDebugFile,
                    SymCacheErrorKind::MissingDebugSection => {
                        SymbolicErrorCode::SymCacheErrorMissingDebugSection
                    }
                    SymCacheErrorKind::MissingDebugInfo => {
                        SymbolicErrorCode::SymCacheErrorMissingDebugInfo
                    }
                    SymCacheErrorKind::UnsupportedDebugKind => {
                        SymbolicErrorCode::SymCacheErrorUnsupportedDebugKind
                    }
                    SymCacheErrorKind::ValueTooLarge => {
                        SymbolicErrorCode::SymCacheErrorValueTooLarge
                    }
                    SymCacheErrorKind::WriteFailed => SymbolicErrorCode::SymCacheErrorWriteFailed,
                };
            }
        }

        SymbolicErrorCode::Unknown
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
            SymbolicErrorCode::from_error(err)
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
    LAST_ERROR.with(|e| {
        if let Some(ref err) = *e.borrow() {
            let mut msg = err.to_string();
            for cause in err.causes().skip(1) {
                write!(&mut msg, "\n  caused by: {}", cause).ok();
            }
            SymbolicStr::from_string(msg)
        } else {
            Default::default()
        }
    })
}

/// Returns the panic information as string.
#[no_mangle]
pub unsafe extern "C" fn symbolic_err_get_backtrace() -> SymbolicStr {
    LAST_ERROR.with(|e| {
        if let Some(ref error) = *e.borrow() {
            let backtrace = error.backtrace().to_string();
            if !backtrace.is_empty() {
                use std::fmt::Write;
                let mut out = String::new();
                write!(&mut out, "stacktrace: {}", backtrace).ok();
                SymbolicStr::from_string(out)
            } else {
                Default::default()
            }
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
    let uuid = Uuid::from_bytes(&(*uuid).data[..]).unwrap_or_default();
    SymbolicStr::from_string(uuid.hyphenated().to_string())
}
