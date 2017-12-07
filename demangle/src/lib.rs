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
//! # extern crate symbolic_demangle;
//! # extern crate symbolic_common;
//! # use symbolic_demangle::Symbol;
//! # use symbolic_common::Language;
//! # fn main() {
//! let sym = Symbol::new("__ZN3std2io4Read11read_to_end17hb85a0f6802e14499E");
//! assert_eq!(sym.language(), Some(Language::Rust));
//! assert_eq!(sym.to_string(), "std::io::Read::read_to_end");
//! # }
//! ```
extern crate symbolic_common;
extern crate rustc_demangle;

use symbolic_common::{ErrorKind, Result, Language};
use std::fmt;
use std::ptr;
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
    fn symbolic_demangle_cpp(
        sym: *const c_char,
        buf_out: *mut *mut c_char,
    ) -> c_int;
    fn symbolic_demangle_cpp_free(buf: *mut c_char);
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

fn is_maybe_objc(ident: &str) -> bool {
    (ident.starts_with("-[") || ident.starts_with("+[")) &&
        ident.ends_with("]")
}

fn is_maybe_cpp(ident: &str) -> bool {
    ident.starts_with("_Z") || ident.starts_with("__Z")
}


fn try_demangle_cpp(ident: &str, opts: &DemangleOptions) -> Result<Option<String>> {
    let ident = unsafe {
        let mut buf_out = ptr::null_mut();
        let sym = CString::new(ident.replace("\x00", "")).unwrap();
        let rv = symbolic_demangle_cpp(sym.as_ptr(), &mut buf_out);
        if rv == 0 {
            return Err(ErrorKind::BadSymbol("not a valid C++ identifier".into()).into());
        }
        let rv = CStr::from_ptr(buf_out).to_string_lossy().into_owned();
        symbolic_demangle_cpp_free(buf_out);
        rv
    };

    if opts.with_arguments {
        Ok(Some(ident))
    } else {
        Ok(ident.split('(').next().map(|x| x.to_string()))
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

fn try_demangle_objc(ident: &str, _opts: &DemangleOptions) -> Result<Option<String>> {
    Ok(Some(ident.to_string()))
}

fn try_demangle_objcpp(ident: &str, opts: &DemangleOptions) -> Result<Option<String>> {
    if is_maybe_objc(ident) {
        try_demangle_objc(ident, opts)
    } else if is_maybe_cpp(ident) {
        try_demangle_cpp(ident, opts)
    } else {
        Ok(None)
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
    lang: Option<Language>,
}

impl<'a> Symbol<'a> {
    /// Constructs a new mangled symbol.
    pub fn new(mangled: &'a str) -> Symbol<'a> {
        Symbol { mangled: mangled, lang: None }
    }

    /// Constructs a new mangled symbol with known language.
    pub fn with_language(mangled: &'a str, lang: Language) -> Symbol<'a> {
        let lang_opt = match lang {
            // Ignore unknown languages and apply heuristics instead
            Language::Unknown | Language::__Max => None,
            _ => Some(lang),
        };

        Symbol { mangled: mangled, lang: lang_opt }
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
        if let Some(lang) = self.lang {
            return Some(lang);
        }

        if is_maybe_objc(self.mangled) {
            return Some(Language::ObjC);
        }

        if rustc_demangle::try_demangle(self.mangled).is_ok() {
            return Some(Language::Rust);
        }

        if is_maybe_cpp(self.mangled) {
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
        use Language::*;
        match self.language() {
            Some(ObjC) => try_demangle_objc(self.mangled, opts),
            Some(ObjCpp) => try_demangle_objcpp(self.mangled, opts),
            Some(Rust) => try_demangle_rust(self.mangled, opts),
            Some(Cpp) => try_demangle_cpp(self.mangled, opts),
            Some(Swift) => try_demangle_swift(self.mangled, opts),
            _ => Ok(None),
        }
    }
}

impl<'a> fmt::Display for Symbol<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Ok(Some(sym)) = self.demangle(&DemangleOptions {
            with_arguments: f.alternate(),
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
