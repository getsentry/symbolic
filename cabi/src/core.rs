use std::borrow::Cow;
use std::ffi::CStr;
use std::mem;
use std::os::raw::c_char;
use std::ptr;
use std::slice;
use std::str;

use failure::Error;
use uuid::Uuid;

use crate::utils::{set_panic_hook, Panic, LAST_ERROR};

/// CABI wrapper around a Rust string.
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
    pub fn as_str(&self) -> &str {
        unsafe { str::from_utf8_unchecked(slice::from_raw_parts(self.data as *const _, self.len)) }
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
    pub data: [u8; 16],
}

impl SymbolicUuid {
    /// Creates a new `SymbolicUuid` from a raw uuid.
    pub fn new(uuid: Uuid) -> SymbolicUuid {
        unsafe { mem::transmute(*uuid.as_bytes()) }
    }

    /// Returns the Rust UUID managed by a `SymbolicUUID`.
    pub fn as_uuid(&self) -> &Uuid {
        unsafe { &*(self as *const Self as *const Uuid) }
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
    ObjectErrorUnsupportedObject = 2101,
    ObjectErrorBadBreakpadObject = 2102,
    ObjectErrorBadElfObject = 2103,
    ObjectErrorBadMachOObject = 2104,
    ObjectErrorBadPdbObject = 2105,
    ObjectErrorBadPeObject = 2106,
    DwarfErrorInvalidUnitRef = 2201,
    DwarfErrorInvalidFileRef = 2202,
    DwarfErrorUnexpectedInline = 2203,
    DwarfErrorInvertedFunctionRange = 2204,
    DwarfErrorCorruptedData = 2205,

    // symbolic::minidump::cfi
    CfiErrorMissingDebugInfo = 3001,
    CfiErrorUnsupportedDebugFormat = 3002,
    CfiErrorBadDebugInfo = 3003,
    CfiErrorUnsupportedArch = 3004,
    CfiErrorWriteError = 3005,
    CfiErrorBadFileMagic = 3006,
    CfiErrorInvalidAddress = 3007,

    // symbolic::minidump::processor
    ProcessMinidumpErrorMinidumpNotFound = 4001,
    ProcessMinidumpErrorNoMinidumpHeader = 4002,
    ProcessMinidumpErrorNoThreadList = 4003,
    ProcessMinidumpErrorInvalidThreadIndex = 4004,
    ProcessMinidumpErrorInvalidThreadId = 4005,
    ProcessMinidumpErrorDuplicateRequestingThreads = 4006,
    ProcessMinidumpErrorSymbolSupplierInterrupted = 4007,

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
    SymCacheErrorTooManyValues = 6012,

    // symbolic::unreal
    Unreal4ErrorEmpty = 7002,
    Unreal4ErrorBadCompression = 7004,
    Unreal4ErrorInvalidXml = 7005,
    Unreal4ErrorInvalidLogEntry = 7006,
    Unreal4ErrorBadData = 7007,
    Unreal4ErrorTrailingData = 7008,

    // apple-crash-report-parser
    AppleCrashReportParseErrorIo = 8001,
    AppleCrashReportParseErrorInvalidIncidentIdentifier = 8002,
    AppleCrashReportParseErrorInvalidReportVersion = 8003,
    AppleCrashReportParseErrorInvalidTimestamp = 8004,
}

impl SymbolicErrorCode {
    /// This maps all errors that can possibly happen.
    #[allow(clippy::cyclomatic_complexity)]
    pub fn from_error(error: &Error) -> SymbolicErrorCode {
        for cause in error.iter_chain() {
            if cause.downcast_ref::<Panic>().is_some() {
                return SymbolicErrorCode::Panic;
            }

            use std::io::Error as IoError;
            if cause.downcast_ref::<IoError>().is_some() {
                return SymbolicErrorCode::IoError;
            }

            use symbolic::common::{ParseDebugIdError, UnknownArchError, UnknownLanguageError};
            if cause.downcast_ref::<UnknownArchError>().is_some() {
                return SymbolicErrorCode::UnknownArchError;
            } else if cause.downcast_ref::<UnknownLanguageError>().is_some() {
                return SymbolicErrorCode::UnknownLanguageError;
            } else if cause.downcast_ref::<ParseDebugIdError>().is_some() {
                return SymbolicErrorCode::ParseDebugIdError;
            }

            use symbolic::debuginfo::{
                dwarf::DwarfErrorKind, ObjectError, UnknownFileFormatError, UnknownObjectKindError,
            };
            if cause.downcast_ref::<UnknownObjectKindError>().is_some() {
                return SymbolicErrorCode::UnknownObjectKindError;
            } else if cause.downcast_ref::<UnknownFileFormatError>().is_some() {
                return SymbolicErrorCode::UnknownFileFormatError;
            } else if let Some(error) = cause.downcast_ref::<ObjectError>() {
                return match error {
                    ObjectError::UnsupportedObject => {
                        SymbolicErrorCode::ObjectErrorUnsupportedObject
                    }
                    ObjectError::Breakpad(_) => SymbolicErrorCode::ObjectErrorBadBreakpadObject,
                    ObjectError::Elf(_) => SymbolicErrorCode::ObjectErrorBadElfObject,
                    ObjectError::MachO(_) => SymbolicErrorCode::ObjectErrorBadMachOObject,
                    ObjectError::Pdb(_) => SymbolicErrorCode::ObjectErrorBadPdbObject,
                    ObjectError::Pe(_) => SymbolicErrorCode::ObjectErrorBadPeObject,
                    ObjectError::Dwarf(ref e) => match e.kind() {
                        DwarfErrorKind::InvalidUnitRef(_) => {
                            SymbolicErrorCode::DwarfErrorInvalidUnitRef
                        }
                        DwarfErrorKind::InvalidFileRef(_) => {
                            SymbolicErrorCode::DwarfErrorInvalidFileRef
                        }
                        DwarfErrorKind::UnexpectedInline => {
                            SymbolicErrorCode::DwarfErrorUnexpectedInline
                        }
                        DwarfErrorKind::InvertedFunctionRange => {
                            SymbolicErrorCode::DwarfErrorInvertedFunctionRange
                        }
                        DwarfErrorKind::CorruptedData => SymbolicErrorCode::DwarfErrorCorruptedData,
                    },
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
                    CfiErrorKind::InvalidAddress => SymbolicErrorCode::CfiErrorInvalidAddress,
                    CfiErrorKind::WriteError => SymbolicErrorCode::CfiErrorWriteError,
                    CfiErrorKind::BadFileMagic => SymbolicErrorCode::CfiErrorBadFileMagic,
                };
            }

            use symbolic::minidump::processor::{ProcessMinidumpError, ProcessResult};
            if let Some(error) = cause.downcast_ref::<ProcessMinidumpError>() {
                return match error.kind() {
                    // `Ok` is not used in errors
                    ProcessResult::Ok => SymbolicErrorCode::Unknown,
                    ProcessResult::MinidumpNotFound => {
                        SymbolicErrorCode::ProcessMinidumpErrorMinidumpNotFound
                    }
                    ProcessResult::NoMinidumpHeader => {
                        SymbolicErrorCode::ProcessMinidumpErrorNoMinidumpHeader
                    }
                    ProcessResult::NoThreadList => {
                        SymbolicErrorCode::ProcessMinidumpErrorNoThreadList
                    }
                    ProcessResult::InvalidThreadIndex => {
                        SymbolicErrorCode::ProcessMinidumpErrorInvalidThreadIndex
                    }
                    ProcessResult::InvalidThreadId => {
                        SymbolicErrorCode::ProcessMinidumpErrorInvalidThreadId
                    }
                    ProcessResult::DuplicateRequestingThreads => {
                        SymbolicErrorCode::ProcessMinidumpErrorDuplicateRequestingThreads
                    }
                    ProcessResult::SymbolSupplierInterrupted => {
                        SymbolicErrorCode::ProcessMinidumpErrorSymbolSupplierInterrupted
                    }
                };
            }

            use symbolic::sourcemap::ParseSourceMapError;
            if cause.downcast_ref::<ParseSourceMapError>().is_some() {
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
                    SymCacheErrorKind::ValueTooLarge(_) => {
                        SymbolicErrorCode::SymCacheErrorValueTooLarge
                    }
                    SymCacheErrorKind::WriteFailed => SymbolicErrorCode::SymCacheErrorWriteFailed,
                    SymCacheErrorKind::TooManyValues(_) => {
                        SymbolicErrorCode::SymCacheErrorTooManyValues
                    }
                };
            }

            use symbolic::unreal::Unreal4Error;
            if let Some(error) = cause.downcast_ref::<Unreal4Error>() {
                return match error {
                    Unreal4Error::Empty => SymbolicErrorCode::Unreal4ErrorEmpty,
                    Unreal4Error::BadCompression(_) => {
                        SymbolicErrorCode::Unreal4ErrorBadCompression
                    }
                    Unreal4Error::BadData(_) => SymbolicErrorCode::Unreal4ErrorBadData,
                    Unreal4Error::TrailingData => SymbolicErrorCode::Unreal4ErrorTrailingData,
                    Unreal4Error::InvalidXml(_) => SymbolicErrorCode::Unreal4ErrorInvalidXml,
                    Unreal4Error::InvalidLogEntry(_) => {
                        SymbolicErrorCode::Unreal4ErrorInvalidLogEntry
                    }
                };
            }

            use apple_crash_report_parser::ParseError;
            if let Some(error) = cause.downcast_ref::<ParseError>() {
                return match error {
                    ParseError::Io(_) => SymbolicErrorCode::AppleCrashReportParseErrorIo,
                    ParseError::InvalidIncidentIdentifier(_) => {
                        SymbolicErrorCode::AppleCrashReportParseErrorInvalidIncidentIdentifier
                    }
                    ParseError::InvalidReportVersion(_) => {
                        SymbolicErrorCode::AppleCrashReportParseErrorInvalidReportVersion
                    }
                    ParseError::InvalidTimestamp(_) => {
                        SymbolicErrorCode::AppleCrashReportParseErrorInvalidTimestamp
                    }
                };
            }
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
            for cause in err.iter_causes() {
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
    /// Creates a symbolic string from a raw C string.
    ///
    /// This sets the string to owned.  In case it's not owned you either have
    /// to make sure you are not freeing the memory or you need to set the
    /// owned flag to false.
    unsafe fn symbolic_str_from_cstr(string: *const c_char) -> Result<SymbolicStr> {
        let s = CStr::from_ptr(string).to_str()?;
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
    SymbolicStr::from_string(uuid.to_hyphenated_ref().to_string())
}
