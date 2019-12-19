//! Provides demangling support.
//!
//! Currently supported languages:
//!
//! * C++ (without windows)
//! * Rust
//! * Swift
//! * ObjC (only symbol detection)
//!
//! As the demangling schemes for different languages are different, the
//! feature set is also inconsistent.  In particular Rust for instance has
//! no overloading so argument types are generally not expected to be
//! encoded into the function name whereas they are in Swift and C++.
//!
//! ## Examples
//!
//! ```rust
//! use symbolic_common::{Language, Name};
//! use symbolic_demangle::Demangle;
//!
//! let name = Name::new("__ZN3std2io4Read11read_to_end17hb85a0f6802e14499E");
//! assert_eq!(name.detect_language(), Language::Rust);
//! assert_eq!(name.try_demangle(Default::default()), "std::io::Read::read_to_end");
//! ```

#![warn(missing_docs)]

use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

use cpp_demangle::{DemangleOptions as CppOptions, Symbol as CppSymbol};
use msvc_demangler::DemangleFlags as MsvcFlags;

use symbolic_common::{Language, Name};

extern "C" {
    fn symbolic_demangle_swift(
        sym: *const c_char,
        buf: *mut c_char,
        buf_len: usize,
        simplified: c_int,
    ) -> c_int;

    fn symbolic_demangle_is_swift_symbol(sym: *const c_char) -> c_int;
}

/// Defines the output format of the demangler.
#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum DemangleFormat {
    /// Strips parameter names and sometimes packages or namespaces.
    Short,

    /// Outputs the full demangled name.
    Full,
}

/// Options for the demangling.
#[derive(Debug, Copy, Clone)]
pub struct DemangleOptions {
    /// Format to use for the output.
    pub format: DemangleFormat,

    /// Should arguments be returned.
    ///
    /// The default behavior is that arguments are not included in the
    /// demangled output, however they are if you convert the symbol
    /// into a string.
    pub with_arguments: bool,
}

impl Default for DemangleOptions {
    fn default() -> DemangleOptions {
        DemangleOptions {
            format: DemangleFormat::Short,
            with_arguments: false,
        }
    }
}

fn is_maybe_objc(ident: &str) -> bool {
    (ident.starts_with("-[") || ident.starts_with("+[")) && ident.ends_with(']')
}

fn is_maybe_cpp(ident: &str) -> bool {
    ident.starts_with("_Z") || ident.starts_with("__Z")
}

fn is_maybe_msvc(ident: &str) -> bool {
    ident.starts_with('?') || ident.starts_with("@?")
}

fn is_maybe_switf(ident: &str) -> bool {
    CString::new(ident)
        .map(|cstr| unsafe { symbolic_demangle_is_swift_symbol(cstr.as_ptr()) != 0 })
        .unwrap_or(false)
}

fn try_demangle_msvc(ident: &str, opts: DemangleOptions) -> Option<String> {
    let flags = match opts.format {
        DemangleFormat::Full => MsvcFlags::COMPLETE,
        DemangleFormat::Short => {
            if opts.with_arguments {
                MsvcFlags::NO_FUNCTION_RETURNS
            } else {
                MsvcFlags::NAME_ONLY
            }
        }
    };

    msvc_demangler::demangle(ident, flags).ok()
}

fn try_demangle_cpp(ident: &str, opts: DemangleOptions) -> Option<String> {
    if is_maybe_msvc(ident) {
        return try_demangle_msvc(ident, opts);
    }

    let symbol = match CppSymbol::new(ident) {
        Ok(symbol) => symbol,
        Err(_) => return None,
    };

    let opts = CppOptions {
        no_params: !opts.with_arguments,
    };

    match symbol.demangle(&opts) {
        Ok(demangled) => Some(demangled),
        Err(_) => None,
    }
}

fn try_demangle_rust(ident: &str, _opts: DemangleOptions) -> Option<String> {
    match rustc_demangle::try_demangle(ident) {
        Ok(demangled) => Some(format!("{:#}", demangled)),
        Err(_) => None,
    }
}

fn try_demangle_swift(ident: &str, opts: DemangleOptions) -> Option<String> {
    let mut buf = vec![0 as c_char; 4096];
    let sym = match CString::new(ident) {
        Ok(sym) => sym,
        Err(_) => return None,
    };

    let simplified = match opts.format {
        DemangleFormat::Short => {
            if opts.with_arguments {
                1
            } else {
                2
            }
        }
        DemangleFormat::Full => 0,
    };

    unsafe {
        match symbolic_demangle_swift(sym.as_ptr(), buf.as_mut_ptr(), buf.len(), simplified) {
            0 => None,
            _ => Some(CStr::from_ptr(buf.as_ptr()).to_string_lossy().to_string()),
        }
    }
}

fn try_demangle_objc(ident: &str, _opts: DemangleOptions) -> Option<String> {
    Some(ident.to_string())
}

fn try_demangle_objcpp(ident: &str, opts: DemangleOptions) -> Option<String> {
    if is_maybe_objc(ident) {
        try_demangle_objc(ident, opts)
    } else if is_maybe_cpp(ident) {
        try_demangle_cpp(ident, opts)
    } else {
        None
    }
}

/// Allows to demangle potentially mangled names.
///
/// Non-mangled names are largely ignored and language detection will not
/// return a language. Upon formatting, the symbol is automatically demangled
/// (without arguments).
pub trait Demangle {
    /// Infers the language of a mangled name.
    ///
    /// In case the symbol is not mangled or not one of the supported languages
    /// the return value will be `None`. If the language of the symbol was
    /// specified explicitly, this is returned instead.
    fn detect_language(&self) -> Language;

    /// Demangles the name with the given options.
    fn demangle(&self, opts: DemangleOptions) -> Option<String>;

    /// Tries to demangle the name and falls back to the original name.
    fn try_demangle(&self, opts: DemangleOptions) -> Cow<'_, str>;
}

impl<'a> Demangle for Name<'a> {
    fn detect_language(&self) -> Language {
        if self.language() != Language::Unknown {
            return self.language();
        }

        if is_maybe_objc(self.as_str()) {
            return Language::ObjC;
        }

        if rustc_demangle::try_demangle(self.as_str()).is_ok() {
            return Language::Rust;
        }

        if is_maybe_cpp(self.as_str()) || is_maybe_msvc(self.as_str()) {
            return Language::Cpp;
        }

        if is_maybe_switf(self.as_str()) {
            return Language::Swift;
        }

        Language::Unknown
    }

    fn demangle(&self, opts: DemangleOptions) -> Option<String> {
        match self.detect_language() {
            Language::ObjC => try_demangle_objc(self.as_str(), opts),
            Language::ObjCpp => try_demangle_objcpp(self.as_str(), opts),
            Language::Rust => try_demangle_rust(self.as_str(), opts),
            Language::Cpp => try_demangle_cpp(self.as_str(), opts),
            Language::Swift => try_demangle_swift(self.as_str(), opts),
            _ => None,
        }
    }

    fn try_demangle(&self, opts: DemangleOptions) -> Cow<'_, str> {
        match self.demangle(opts) {
            Some(demangled) => Cow::Owned(demangled),
            None => Cow::Borrowed(self.as_str()),
        }
    }
}

/// Demangles an identifier and falls back to the original symbol.
///
/// This is a shortcut for using ``Name::try_demangle``.
///
/// ```
/// # use symbolic_demangle::*;
/// let rv = demangle("_ZN3foo3barE");
/// assert_eq!(&rv, "foo::bar");
/// ```
pub fn demangle(ident: &str) -> Cow<'_, str> {
    match Name::new(ident).demangle(Default::default()) {
        Some(demangled) => Cow::Owned(demangled),
        None => Cow::Borrowed(ident),
    }
}
