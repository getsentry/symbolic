use symbolic::common::Name;
use symbolic::demangle::{Demangle, DemangleFormat, DemangleOptions};

use crate::core::SymbolicStr;

/// Creates a name from a string passed via FFI.
unsafe fn get_name(ident: *const SymbolicStr, lang: *const SymbolicStr) -> Name<'static> {
    if lang.is_null() {
        Name::new((*ident).as_str())
    } else {
        let lang = (*lang).as_str().parse().unwrap_or_default();
        Name::with_language((*ident).as_str(), lang)
    }
}

ffi_fn! {
    /// Demangles a given identifier.
    ///
    /// This demangles with the default behavior in symbolic. If no language
    /// is specified, it will be auto-detected.
    unsafe fn symbolic_demangle(
        ident: *const SymbolicStr,
        lang: *const SymbolicStr,
    ) -> Result<SymbolicStr> {
        let name = get_name(ident, lang);
        let demangled = name.try_demangle(DemangleOptions {
            with_arguments: true,
            format: DemangleFormat::Short,
            None,
        });

        Ok(demangled.into_owned().into())
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
        let name = get_name(ident, lang);
        let demangled = name.try_demangle(DemangleOptions {
            with_arguments: false,
            format: DemangleFormat::Short,
            None,
        });

        Ok(demangled.into_owned().into())
    }
}
