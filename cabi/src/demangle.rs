use symbolic_demangle::Symbol;
use symbolic_common::Language;

use core::SymbolicStr;

ffi_fn! {
    /// Demangles a given identifier.
    ///
    /// This demangles with the default behavior in symbolic. If no language
    /// is specified, it will be auto-detected.
    unsafe fn symbolic_demangle(
        ident: *const SymbolicStr,
        lang: *const SymbolicStr,
    ) -> Result<SymbolicStr> {
        let sym = if lang.is_null() {
            Symbol::new((*ident).as_str())
        } else {
            let lang = Language::parse((*lang).as_str());
            Symbol::with_language((*ident).as_str(), lang)
        };

        Ok(SymbolicStr::from_string(format!("{:#}", sym)))
    }
}

ffi_fn! {
    /// Demangles a given identifier.
    ///
    /// This is similar to `symbolic_demangle` but does not demangle the
    /// arguments and instead strips them. If no language is specified, it
    /// will be auto-detected.
    unsafe fn symbolic_demangle_no_args(
        ident: *const SymbolicStr,
        lang: *const SymbolicStr,
    ) -> Result<SymbolicStr> {
        let sym = if lang.is_null() {
            Symbol::new((*ident).as_str())
        } else {
            let lang = Language::parse((*lang).as_str());
            Symbol::with_language((*ident).as_str(), lang)
        };

        Ok(SymbolicStr::from_string(format!("{}", sym)))
    }
}
