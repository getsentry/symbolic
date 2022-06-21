use std::borrow::Cow;
use std::ffi::CStr;
use std::mem;
use std::os::raw::c_char;
use std::ptr;
use std::slice;
use std::str;

use symbolic::common::Uuid;

use crate::utils::{set_panic_hook, Panic, LAST_ERROR};

/// A length-prefixed UTF-8 string.
///
/// As opposed to C strings, this string is not null-terminated. If the string is owned, indicated
/// by the `owned` flag, the owner must call the `free` function on this string. The convention is:
///
///  - When obtained as instance through return values, always free the string.
///  - When obtained as pointer through field access, never free the string.
#[repr(C)]
pub struct SymbolicStr {
    /// Pointer to the UTF-8 encoded string data.
    pub data: *mut c_char,
    /// The length of the string pointed to by `data`.
    pub len: usize,
    /// Indicates that the string is owned and must be freed.
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
    /// Creates a new `SymbolicStr` from a Rust string.
    pub fn new(s: &str) -> SymbolicStr {
        SymbolicStr {
            data: s.as_ptr() as *mut c_char,
            len: s.len(),
            owned: false,
        }
    }

    /// Creates a new `SymbolicStr` from an owned Rust string.
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

    /// Releases memory held by an unmanaged `SymbolicStr`.
    pub unsafe fn free(&mut self) {
        if self.owned {
            String::from_raw_parts(self.data as *mut _, self.len, self.len);
            self.data = ptr::null_mut();
            self.len = 0;
            self.owned = false;
        }
    }

    /// Returns the Rust string managed by a `SymbolicStr`.
    pub unsafe fn as_str(&self) -> &str {
        str::from_utf8_unchecked(slice::from_raw_parts(self.data as *const _, self.len))
    }
}

impl Drop for SymbolicStr {
    fn drop(&mut self) {
        unsafe { self.free() }
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

impl<'a> From<Cow<'a, str>> for SymbolicStr {
    fn from(cow: Cow<'a, str>) -> SymbolicStr {
        match cow {
            Cow::Borrowed(string) => SymbolicStr::new(string),
            Cow::Owned(string) => SymbolicStr::from_string(string),
        }
    }
}

/// CABI wrapper around a UUID.
#[repr(C)]
pub struct SymbolicUuid {
    /// UUID bytes in network byte order (big endian).
    pub data: [u8; 16],
}

impl SymbolicUuid {
    /// Creates a new `SymbolicUuid` from a raw uuid.
    pub fn new(uuid: Uuid) -> SymbolicUuid {
        unsafe { mem::transmute(*uuid.as_bytes()) }
    }

    /// Returns the Rust UUID managed by a `SymbolicUUID`.
    pub unsafe fn as_uuid(&self) -> &Uuid {
        &*(self as *const Self as *const Uuid)
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

    // // std::io
    IoError = 101,

    // symbolic::common
    UnknownArchError = 1001,
    UnknownLanguageError = 1002,
    ParseDebugIdError = 1003,

    // symbolic::debuginfo
    UnknownObjectKindError = 2001,
    UnknownFileFormatError = 2002,
    ObjectErrorUnknown = 2100,
    ObjectErrorUnsupportedObject = 2101,
    ObjectErrorBadBreakpadObject = 2102,
    ObjectErrorBadElfObject = 2103,
    ObjectErrorBadMachOObject = 2104,
    ObjectErrorBadPdbObject = 2105,
    ObjectErrorBadPeObject = 2106,
    ObjectErrorBadSourceBundle = 2107,
    ObjectErrorBadWasmObject = 2108,
    DwarfErrorUnknown = 2200,
    DwarfErrorInvalidUnitRef = 2201,
    DwarfErrorInvalidFileRef = 2202,
    DwarfErrorUnexpectedInline = 2203,
    DwarfErrorInvertedFunctionRange = 2204,
    DwarfErrorCorruptedData = 2205,

    // symbolic::cfi
    CfiErrorUnknown = 3000,
    CfiErrorMissingDebugInfo = 3001,
    CfiErrorUnsupportedDebugFormat = 3002,
    CfiErrorBadDebugInfo = 3003,
    CfiErrorUnsupportedArch = 3004,
    CfiErrorWriteError = 3005,
    CfiErrorBadFileMagic = 3006,
    CfiErrorInvalidAddress = 3007,

    // sourcemap
    ParseSourceMapError = 5001,

    // symbolic::symcache
    SymCacheErrorUnknown = 6000,
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
    SymCacheErrorTooManyValues = 6012,
}

impl SymbolicErrorCode {
    /// This maps all errors that can possibly happen.
    pub fn from_error(error: &(dyn std::error::Error + 'static)) -> SymbolicErrorCode {
        let mut source = Some(error);

        while let Some(error) = source {
            if error.downcast_ref::<Panic>().is_some() {
                return SymbolicErrorCode::Panic;
            }

            use std::io::Error as IoError;
            if error.downcast_ref::<IoError>().is_some() {
                return SymbolicErrorCode::IoError;
            }

            use symbolic::common::{ParseDebugIdError, UnknownArchError, UnknownLanguageError};
            if error.downcast_ref::<UnknownArchError>().is_some() {
                return SymbolicErrorCode::UnknownArchError;
            } else if error.downcast_ref::<UnknownLanguageError>().is_some() {
                return SymbolicErrorCode::UnknownLanguageError;
            } else if error.downcast_ref::<ParseDebugIdError>().is_some() {
                return SymbolicErrorCode::ParseDebugIdError;
            }

            use symbolic::debuginfo::{
                ObjectError, UnknownFileFormatError, UnknownObjectKindError,
            };
            if error.downcast_ref::<UnknownObjectKindError>().is_some() {
                return SymbolicErrorCode::UnknownObjectKindError;
            } else if error.downcast_ref::<UnknownFileFormatError>().is_some() {
                return SymbolicErrorCode::UnknownFileFormatError;
            } else if error.downcast_ref::<ObjectError>().is_some() {
                return SymbolicErrorCode::ObjectErrorUnknown;
            }

            use symbolic::cfi::{CfiError, CfiErrorKind};
            if let Some(error) = error.downcast_ref::<CfiError>() {
                return match error.kind() {
                    CfiErrorKind::MissingDebugInfo => SymbolicErrorCode::CfiErrorMissingDebugInfo,
                    CfiErrorKind::UnsupportedDebugFormat => {
                        SymbolicErrorCode::CfiErrorUnsupportedDebugFormat
                    }
                    CfiErrorKind::BadDebugInfo => SymbolicErrorCode::CfiErrorBadDebugInfo,
                    CfiErrorKind::UnsupportedArch => SymbolicErrorCode::CfiErrorUnsupportedArch,
                    CfiErrorKind::InvalidAddress => SymbolicErrorCode::CfiErrorInvalidAddress,
                    CfiErrorKind::WriteFailed => SymbolicErrorCode::CfiErrorWriteError,
                    CfiErrorKind::BadFileMagic => SymbolicErrorCode::CfiErrorBadFileMagic,
                    _ => SymbolicErrorCode::CfiErrorUnknown,
                };
            }

            if error.downcast_ref::<sourcemap::Error>().is_some() {
                return SymbolicErrorCode::ParseSourceMapError;
            }

            use symbolic::symcache::{Error, ErrorKind};
            if let Some(error) = error.downcast_ref::<Error>() {
                return match error.kind() {
                    ErrorKind::WrongFormat => SymbolicErrorCode::SymCacheErrorBadFileMagic,
                    ErrorKind::HeaderTooSmall => SymbolicErrorCode::SymCacheErrorBadFileHeader,
                    ErrorKind::WrongVersion => SymbolicErrorCode::SymCacheErrorUnsupportedVersion,
                    ErrorKind::BadDebugFile => SymbolicErrorCode::SymCacheErrorBadDebugFile,
                    _ => SymbolicErrorCode::SymCacheErrorUnknown,
                };
            }

            source = error.source();
        }

        SymbolicErrorCode::Unknown
    }
}

/// Initializes the symbolic library.
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
            SymbolicErrorCode::from_error(err.as_ref())
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
            let mut err = err.as_ref();
            let mut msg = err.to_string();
            while let Some(cause) = err.source() {
                write!(&mut msg, "\n  caused by: {}", cause).ok();
                err = cause;
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
    SymbolicStr::default()
}

/// Clears the last error.
#[no_mangle]
pub unsafe extern "C" fn symbolic_err_clear() {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = None;
    });
}

ffi_fn! {
    /// Creates a symbolic string from a raw C string.
    unsafe fn symbolic_str_from_cstr(string: *const c_char) -> Result<SymbolicStr> {
        let s = CStr::from_ptr(string).to_str()?;
        Ok(SymbolicStr {
            data: s.as_ptr() as *mut _,
            len: s.len(),
            owned: false,
        })
    }
}

/// Frees a symbolic str.
///
/// If the string is marked as not owned then this function does not
/// do anything.
#[no_mangle]
pub unsafe extern "C" fn symbolic_str_free(string: *mut SymbolicStr) {
    (*string).free()
}

/// Returns true if the uuid is nil.
#[no_mangle]
pub unsafe extern "C" fn symbolic_uuid_is_nil(uuid: *const SymbolicUuid) -> bool {
    if let Ok(uuid) = Uuid::from_slice(&(*uuid).data[..]) {
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
    let uuid = Uuid::from_slice(&(*uuid).data[..]).unwrap_or_default();
    SymbolicStr::from_string(uuid.hyphenated().to_string())
}
