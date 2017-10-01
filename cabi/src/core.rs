use std::ptr;
use std::ffi::CString;
use std::os::raw::c_char;

use utils::LAST_ERROR;

use symbolic_common::ErrorKind;


/// Indicates the error that ocurred
#[repr(u32)]
pub enum SymbolicErrorCode {
    NoError = 0,
    Panic = 1,
    Internal = 2,
    Msg = 3,
    Unknown = 4,
    Parse = 101,
    Format = 102,
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
/// If there is no error, 0 is returned.  This allocates new memory that needs
/// to be freed with `symbolic_cstr_free`.
#[no_mangle]
pub unsafe extern "C" fn symbolic_err_get_last_message() -> *mut c_char {
    LAST_ERROR.with(|e| {
        if let Some(ref err) = *e.borrow() {
            if let Ok(rv) = CString::new(err.to_string()) {
                return rv.into_raw();
            }
        }
        ptr::null_mut()
    })
}

/// Clears the last error.
#[no_mangle]
pub unsafe extern "C" fn symbolic_err_clear() {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = None;
    });
}

/// Frees a C-string allocated in symbolic.
#[no_mangle]
pub unsafe extern "C" fn symbolic_cstr_free(s: *mut c_char) {
    if !s.is_null() {
        let _ = CString::from_raw(s);
    }
}
