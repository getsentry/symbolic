use symbolic_demangle::Symbol;

use core::SymbolicStr;

ffi_fn! {
    /// Demangles a given identifier.
    ///
    /// This demangles with the default behavior in symbolic.
    unsafe fn symbolic_demangle(ident: *const SymbolicStr) -> Result<SymbolicStr> {
        let sym = Symbol::new((*ident).as_str());
        Ok(SymbolicStr::from_string(format!("{:#}", sym)))
    }
}

ffi_fn! {
    /// Demangles a given identifier.
    ///
    /// This is similar to `symbolic_demangle` but does not demangle the
    /// arguments and instead strips them.
    unsafe fn symbolic_demangle_no_args(ident: *const SymbolicStr) -> Result<SymbolicStr> {
        let sym = Symbol::new((*ident).as_str());
        Ok(SymbolicStr::from_string(format!("{}", sym)))
    }
}
