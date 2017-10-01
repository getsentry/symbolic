use std::os::raw::c_char;
use std::ffi::{CStr, CString};

use symbolic_demangle::Symbol;

ffi_fn! {
    /// Demangles a given identifier.
    ///
    /// This demangles with the default behavior in symbolic.
    unsafe fn symbolic_demangle(ident: *const c_char) -> Result<*mut c_char> {
        let sym = Symbol::new(CStr::from_ptr(ident).to_str()?);
        Ok(CString::new(format!("{:#}", sym))?.into_raw())
    }
}

ffi_fn! {
    /// Demangles a given identifier.
    ///
    /// This is similar to `symbolic_demangle` but does not demangle the
    /// arguments and instead strips them.
    unsafe fn symbolic_demangle_no_args(ident: *const c_char) -> Result<*mut c_char> {
        let sym = Symbol::new(CStr::from_ptr(ident).to_str()?);
        Ok(CString::new(format!("{}", sym))?.into_raw())
    }
}
