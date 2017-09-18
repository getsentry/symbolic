//! Provides demangling support.
//!
//! Currently supported languages:
//!
//! * C++ (without windows)
//! * Rust
//! * Swift
//! * ObjC (only symbol detection)
//!
//! As the demangling schemes for different languages are different the
//! feature set is also inconsistent.  In particular Rust for instance has
//! no overloading so argument types are generally not expected to be
//! encoded into the function name whereas they are in Swift and C++.
//!
//! ## Examples
//!
//! ```rust
//! # use symbolic_demangle::{Symbol, Language};
//! let sym = Symbol::new("__ZN3std2io4Read11read_to_end17hb85a0f6802e14499E");
//! assert_eq!(sym.language(), Some(Language::Rust));
//! assert_eq!(sym.to_string(), "std::io::Read::read_to_end");
//! ```
extern crate symbolic_common;
extern crate rustc_demangle;
extern crate cpp_demangle;

use symbolic_common::{ErrorKind, Result, Language};
use std::fmt;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

extern "C" {
    fn symbolic_demangle_swift(
        sym: *const c_char,
        buf: *mut c_char,
        buf_len: usize,
        simplified: c_int,
    ) -> c_int;
    fn symbolic_demangle_is_swift_symbol(sym: *const c_char) -> c_int;
}

/// Defines the output format of the demangler
#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum DemangleFormat {
    Short,
    Full,
}

/// Options for the demangling
#[derive(Debug, Clone)]
pub struct DemangleOptions {
    /// format to use for the output
    pub format: DemangleFormat,
    /// Should arguments be returned
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

fn try_demangle_cpp(ident: &str, opts: &DemangleOptions) -> Result<Option<String>> {
    match cpp_demangle::Symbol::new(ident) {
        Ok(sym) => Ok(
            sym.demangle(&cpp_demangle::DemangleOptions {
                no_params: !opts.with_arguments,
            }).ok(),
        ),
        Err(err) => Err(ErrorKind::BadSymbol(err.to_string()).into()),
    }
}

fn try_demangle_rust(ident: &str, _opts: &DemangleOptions) -> Result<Option<String>> {
    if let Ok(dm) = rustc_demangle::try_demangle(ident) {
        Ok(Some(format!("{:#}", dm)))
    } else {
        Err(
            ErrorKind::BadSymbol("Not a valid Rust symbol".into()).into(),
        )
    }
}

fn try_demangle_swift(ident: &str, opts: &DemangleOptions) -> Result<Option<String>> {
    let mut buf = vec![0i8; 4096];
    let sym = match CString::new(ident) {
        Ok(sym) => sym,
        Err(_) => {
            return Err(ErrorKind::Internal("embedded null byte").into());
        }
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
            return Ok(None);
        }

        let s = CStr::from_ptr(buf.as_ptr()).to_string_lossy();
        return Ok(Some(s.to_string()));
    }
}

/// Represents a mangled symbol.
///
/// When created from a string this type wraps a potentially mangled
/// symbol.  Non mangled symbols are largely ignored by this type and
/// language checks will not return a language.
///
/// Upon formatting the symbol is automatically demangled (without
/// arguments).
pub struct Symbol<'a> {
    mangled: &'a str,
}

impl<'a> Symbol<'a> {
    /// Constructs a new mangled symbol.
    pub fn new(mangled: &'a str) -> Symbol<'a> {
        Symbol { mangled: mangled }
    }

    /// The raw string of the symbol.
    ///
    /// If the symbol was not mangled this will also return the input data.
    pub fn raw(&self) -> &str {
        self.mangled
    }

    /// The language of the mangled symbol.
    ///
    /// In case the symbol is not mangled or not one of the supported languages
    /// the return value will be `None`.
    pub fn language(&self) -> Option<Language> {
        // objc?
        if (self.mangled.starts_with("-[") || self.mangled.starts_with("+[")) &&
            self.mangled.ends_with("]")
        {
            return Some(Language::ObjC);
        }

        // rust
        if (self.mangled.starts_with("_ZN") || self.mangled.starts_with("__ZN")) &&
            self.mangled.ends_with("E")
        {
            return Some(Language::Rust);
        }

        // c++
        if self.mangled.starts_with("_Z") || self.mangled.starts_with("__Z") {
            return Some(Language::Cpp);
        }

        // swift?
        if let Ok(sym) = CString::new(self.mangled) {
            unsafe {
                if symbolic_demangle_is_swift_symbol(sym.as_ptr()) != 0 {
                    return Some(Language::Swift);
                }
            }
        }

        None
    }

    /// Demangles a symbol with the given options.
    pub fn demangle(&self, opts: &DemangleOptions) -> Result<Option<String>> {
        match self.language() {
            Some(Language::ObjC) => Ok(Some(self.mangled.to_string())),
            Some(Language::Rust) => try_demangle_rust(self.mangled, opts),
            Some(Language::Cpp) => try_demangle_cpp(self.mangled, opts),
            Some(Language::Swift) => try_demangle_swift(self.mangled, opts),
            _ => Ok(None),
        }
    }
}

impl<'a> fmt::Display for Symbol<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Ok(Some(sym)) = self.demangle(&DemangleOptions {
            with_arguments: false,
            ..Default::default()
        }) {
            write!(f, "{}", sym)
        } else {
            write!(f, "{}", self.raw())
        }
    }
}

/// Demangles an identifier.
///
/// This is a shortcut for using ``Symbol::demangle``.
///
/// ```
/// # use symbolic_demangle::*;
/// let rv = demangle("_ZN3foo3barE", &Default::default()).unwrap();
/// assert_eq!(&rv.unwrap(), "foo::bar");
/// ```
pub fn demangle(ident: &str, opts: &DemangleOptions) -> Result<Option<String>> {
    Symbol::new(ident).demangle(opts)
}
