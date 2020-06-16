//! Demangling support for various languages and compilers.
//!
//! Currently supported languages are:
//!
//! - C++ (GCC-style compilers and MSVC)
//! - Rust (both `legacy` and `v0`)
//! - Swift (up to Swift 5.2)
//! - ObjC (only symbol detection)
//!
//! As the demangling schemes for the languages are different, the supported demangling features are
//! inconsistent. For example, argument types were not encoded in legacy Rust mangling and thus not
//! available in demangled names.
//!
//! This module is part of the `symbolic` crate and can be enabled via the `demangle` feature.
//!
//! # Examples
//!
//! ```rust
//! use symbolic_common::{Language, Name};
//! use symbolic_demangle::{Demangle, DemangleOptions};
//!
//! let name = Name::new("__ZN3std2io4Read11read_to_end17hb85a0f6802e14499E");
//! assert_eq!(name.detect_language(), Language::Rust);
//! assert_eq!(name.try_demangle(DemangleOptions::default()), "std::io::Read::read_to_end");
//! ```

#![warn(missing_docs)]

use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

use symbolic_common::{Language, Name};

use cpp_demangle::{DemangleOptions as CppOptions, Symbol as CppSymbol};
use msvc_demangler::DemangleFlags as MsvcFlags;

extern "C" {
    fn symbolic_demangle_swift(
        sym: *const c_char,
        buf: *mut c_char,
        buf_len: usize,
        simplified: c_int,
    ) -> c_int;

    fn symbolic_demangle_is_swift_symbol(sym: *const c_char) -> c_int;
}

/// Defines the output format for demangling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DemangleFormat {
    /// Strips parameter names (in Swift) and sometimes packages or namespaces.
    Short,

    /// Outputs the full demangled name including arguments, return types, generics and packages.
    Full,
}

/// Options for [`Demangle::demangle`].
///
/// [`Demangle::demangle`]: trait.Demangle.html#tymethod.demangle
#[derive(Clone, Copy, Debug)]
pub struct DemangleOptions {
    /// General format to use for the output.
    ///
    /// Defaults to `DemangleFormat::Short`.
    pub format: DemangleFormat,

    /// Determines whether function arguments should be demangled.
    ///
    /// Defaults to `false`.
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
    ident.starts_with("_Z")
        || ident.starts_with("__Z")
        || ident.starts_with("___Z")
        || ident.starts_with("____Z")
}

fn is_maybe_msvc(ident: &str) -> bool {
    ident.starts_with('?') || ident.starts_with("@?")
}

fn is_maybe_swift(ident: &str) -> bool {
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

    let mut cpp_options = CppOptions::new();
    if !opts.with_arguments {
        cpp_options = cpp_options.no_params().no_return_type();
    }

    match symbol.demangle(&cpp_options) {
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

/// An extension trait on `Name` for demangling names.
///
/// See the [module level documentation] for a list of supported languages.
///
/// [module level documentation]: index.html
pub trait Demangle {
    /// Infers the language of a mangled name.
    ///
    /// In case the symbol is not mangled or its language is unknown, the return value will be
    /// `Language::Unknown`. If the language of the symbol was specified explicitly, this is
    /// returned instead. For a list of supported languages, see the [module level documentation].
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_common::{Name, Language};
    /// use symbolic_demangle::{Demangle, DemangleOptions};
    ///
    /// assert_eq!(Name::new("_ZN3foo3barEv").detect_language(), Language::Cpp);
    /// assert_eq!(Name::new("unknown").detect_language(), Language::Unknown);
    /// ```
    ///
    /// [module level documentation]: index.html
    fn detect_language(&self) -> Language;

    /// Demangles the name with the given options.
    ///
    /// Returns `None` in one of the following cases:
    ///  1. The language cannot be detected.
    ///  2. The language is not supported.
    ///  3. Demangling of the name failed.
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_common::Name;
    /// use symbolic_demangle::{Demangle, DemangleOptions};
    ///
    /// assert_eq!(Name::new("_ZN3foo3barEv").demangle(DemangleOptions::default()), Some("foo::bar".to_string()));
    /// assert_eq!(Name::new("unknown").demangle(DemangleOptions::default()), None);
    /// ```
    fn demangle(&self, opts: DemangleOptions) -> Option<String>;

    /// Tries to demangle the name and falls back to the original name.
    ///
    /// Similar to [`demangle`], except that it returns a borrowed instance of the original name if
    /// the name cannot be demangled.
    ///
    /// # Examples
    ///
    /// ```
    /// use symbolic_common::Name;
    /// use symbolic_demangle::{Demangle, DemangleOptions};
    ///
    /// assert_eq!(Name::new("_ZN3foo3barEv").try_demangle(DemangleOptions::default()), "foo::bar");
    /// assert_eq!(Name::new("unknown").try_demangle(DemangleOptions::default()), "unknown");
    /// ```
    ///
    /// [`demangle`]: trait.Demangle.html#tymethod.demangle
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

        if is_maybe_swift(self.as_str()) {
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
/// This is a shortcut for [`Demangle::try_demangle`] with default options.
///
/// # Examples
///
/// ```
/// assert_eq!(symbolic_demangle::demangle("_ZN3foo3barEv"), "foo::bar");
/// ```
///
/// [`Demangle::try_demangle`]: trait.Demangle.html#tymethod.try_demangle
pub fn demangle(ident: &str) -> Cow<'_, str> {
    match Name::new(ident).demangle(Default::default()) {
        Some(demangled) => Cow::Owned(demangled),
        None => Cow::Borrowed(ident),
    }
}
