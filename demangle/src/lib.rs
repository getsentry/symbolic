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
//! # extern crate symbolic_demangle;
//! # extern crate symbolic_common;
//! use symbolic_common::types::{Language, Name};
//! use symbolic_demangle::Demangle;
//!
//! # fn main() {
//! let name = Name::new("__ZN3std2io4Read11read_to_end17hb85a0f6802e14499E");
//! assert_eq!(name.detect_language(), Some(Language::Rust));
//! assert_eq!(&name.try_demangle(Default::default()), "std::io::Read::read_to_end");
//! # }
//! ```
extern crate cpp_demangle;
extern crate rustc_demangle;
extern crate symbolic_common;

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

use cpp_demangle::{DemangleOptions as CppOptions, Symbol};

use symbolic_common::types::{Language, Name};

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
    (ident.starts_with("-[") || ident.starts_with("+[")) && ident.ends_with("]")
}

fn is_maybe_cpp(ident: &str) -> bool {
    ident.starts_with("_Z") || ident.starts_with("__Z")
}

fn try_demangle_cpp(ident: &str, opts: DemangleOptions) -> Option<String> {
    let symbol = match Symbol::new(ident) {
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
        DemangleFormat::Short => if opts.with_arguments {
            1
        } else {
            2
        },
        DemangleFormat::Full => 0,
    };

    unsafe {
        let rv = symbolic_demangle_swift(sym.as_ptr(), buf.as_mut_ptr(), buf.len(), simplified);
        if rv == 0 {
            return None;
        } else {
            let s = CStr::from_ptr(buf.as_ptr()).to_string_lossy();
            Some(s.to_string())
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
    fn detect_language(&self) -> Option<Language>;

    /// Demangles the name with the given options.
    fn demangle(&self, opts: DemangleOptions) -> Option<String>;

    /// Tries to demangle the name and falls back to the original name.
    fn try_demangle(&self, opts: DemangleOptions) -> String;
}

impl<'a> Demangle for Name<'a> {
    fn detect_language(&self) -> Option<Language> {
        if let Some(lang) = self.language() {
            return Some(lang);
        }

        if is_maybe_objc(self.as_str()) {
            return Some(Language::ObjC);
        }

        if rustc_demangle::try_demangle(self.as_str()).is_ok() {
            return Some(Language::Rust);
        }

        if is_maybe_cpp(self.as_str()) {
            return Some(Language::Cpp);
        }

        // swift?
        if let Ok(sym) = CString::new(self.as_str()) {
            unsafe {
                if symbolic_demangle_is_swift_symbol(sym.as_ptr()) != 0 {
                    return Some(Language::Swift);
                }
            }
        }

        None
    }

    fn demangle(&self, opts: DemangleOptions) -> Option<String> {
        use Language::*;
        match self.detect_language() {
            Some(ObjC) => try_demangle_objc(self.as_str(), opts),
            Some(ObjCpp) => try_demangle_objcpp(self.as_str(), opts),
            Some(Rust) => try_demangle_rust(self.as_str(), opts),
            Some(Cpp) => try_demangle_cpp(self.as_str(), opts),
            Some(Swift) => try_demangle_swift(self.as_str(), opts),
            _ => None,
        }
    }

    fn try_demangle(&self, opts: DemangleOptions) -> String {
        match self.demangle(opts) {
            Some(demangled) => demangled,
            None => self.as_str().into(),
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
pub fn demangle(ident: &str) -> String {
    Name::new(ident).try_demangle(Default::default())
}
