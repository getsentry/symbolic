use std::ptr;
use std::ffi::CString;
use std::os::raw::{c_int, c_char};

use utils::{LAST_ERROR, get_error_code_from_kind};

/// Returns the last error code.
///
/// If there is no error, 0 is returned.
#[no_mangle]
pub unsafe extern "C" fn symbolic_err_get_last_code() -> c_int {
    LAST_ERROR.with(|e| {
        if let Some(ref err) = *e.borrow() {
            get_error_code_from_kind(err.kind())
        } else {
            0
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
