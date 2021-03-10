//! Demangling support for various languages and compilers.
//!
//! Currently supported languages are:
//!
//! - C++ (GCC-style compilers and MSVC) (`features = ["cpp", "msvc"]`)
//! - Rust (both `legacy` and `v0`) (`features = ["rust"]`)
//! - Swift (up to Swift 5.3) (`features = ["swift"]`)
//! - ObjC (only symbol detection)
//!
//! As the demangling schemes for the languages are different, the supported demangling features are
//! inconsistent. For example, argument types were not encoded in legacy Rust mangling and thus not
//! available in demangled names.
//! The demangling results should not be considered stable, and may change over time as more
//! demangling features are added.
//!
//! This module is part of the `symbolic` crate and can be enabled via the `demangle` feature.
//!
//! # Examples
//!
//! ```rust
//! # #[cfg(feature = "rust")] {
//! use symbolic_common::{Language, Name};
//! use symbolic_demangle::{Demangle, DemangleOptions};
//!
//! let name = Name::from("__ZN3std2io4Read11read_to_end17hb85a0f6802e14499E");
//! assert_eq!(name.detect_language(), Language::Rust);
//! assert_eq!(name.try_demangle(DemangleOptions::complete()), "std::io::Read::read_to_end");
//! # }
//! ```

#![warn(missing_docs)]

use std::borrow::Cow;
#[cfg(feature = "swift")]
use std::ffi::{CStr, CString};
#[cfg(feature = "swift")]
use std::os::raw::{c_char, c_int};

use symbolic_common::{Language, Name, NameMangling};

#[cfg(feature = "swift")]
const SYMBOLIC_SWIFT_FEATURE_RETURN_TYPE: c_int = 0x1;
#[cfg(feature = "swift")]
const SYMBOLIC_SWIFT_FEATURE_PARAMETERS: c_int = 0x2;

#[cfg(feature = "swift")]
extern "C" {
    fn symbolic_demangle_swift(
        sym: *const c_char,
        buf: *mut c_char,
        buf_len: usize,
        features: c_int,
    ) -> c_int;

    fn symbolic_demangle_is_swift_symbol(sym: *const c_char) -> c_int;
}

/// Options for [`Demangle::demangle`].
///
/// One can chose from complete, or name-only demangling, and toggle specific demangling features
/// explicitly.
///
/// The resulting output depends very much on the language of the mangled [`Name`], and may change
/// over time as more fine grained demangling options and features are added. Not all options are
/// fully supported by each language, and not every feature is mutually exclusive on all languages.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "swift")] {
/// use symbolic_common::{Name, NameMangling, Language};
/// use symbolic_demangle::{Demangle, DemangleOptions};
///
/// let symbol = Name::new("$s8mangling12GenericUnionO3FooyACyxGSicAEmlF", NameMangling::Mangled, Language::Swift);
///
/// let simple = symbol.demangle(DemangleOptions::name_only()).unwrap();
/// assert_eq!(&simple, "GenericUnion.Foo<A>");
///
/// let full = symbol.demangle(DemangleOptions::complete()).unwrap();
/// assert_eq!(&full, "mangling.GenericUnion.Foo<A>(mangling.GenericUnion<A>.Type) -> (Swift.Int) -> mangling.GenericUnion<A>");
/// # }
/// ```
///
/// [`Demangle::demangle`]: trait.Demangle.html#tymethod.demangle
#[derive(Clone, Copy, Debug)]
pub struct DemangleOptions {
    return_type: bool,
    parameters: bool,
}

impl DemangleOptions {
    /// DemangleOptions that output a complete verbose demangling.
    pub const fn complete() -> Self {
        Self {
            return_type: true,
            parameters: true,
        }
    }

    /// DemangleOptions that output the most simple (likely name-only) demangling.
    pub const fn name_only() -> Self {
        Self {
            return_type: false,
            parameters: false,
        }
    }

    /// Determines whether a functions return type should be demangled.
    pub const fn return_type(mut self, return_type: bool) -> Self {
        self.return_type = return_type;
        self
    }

    /// Determines whether function argument types should be demangled.
    pub const fn parameters(mut self, parameters: bool) -> Self {
        self.parameters = parameters;
        self
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

#[cfg(feature = "swift")]
fn is_maybe_swift(ident: &str) -> bool {
    CString::new(ident)
        .map(|cstr| unsafe { symbolic_demangle_is_swift_symbol(cstr.as_ptr()) != 0 })
        .unwrap_or(false)
}

#[cfg(not(feature = "swift"))]
fn is_maybe_swift(_ident: &str) -> bool {
    false
}

#[cfg(feature = "msvc")]
fn try_demangle_msvc(ident: &str, opts: DemangleOptions) -> Option<String> {
    use msvc_demangler::DemangleFlags as MsvcFlags;

    // the flags are bitflags
    let mut flags = MsvcFlags::COMPLETE;
    if !opts.return_type {
        flags |= MsvcFlags::NO_FUNCTION_RETURNS;
    }
    if !opts.parameters {
        // a `NO_ARGUMENTS` flag is there in the code, but commented out
        flags |= MsvcFlags::NAME_ONLY;
    }

    msvc_demangler::demangle(ident, flags).ok()
}

#[cfg(not(feature = "msvc"))]
fn try_demangle_msvc(_ident: &str, _opts: DemangleOptions) -> Option<String> {
    None
}

fn try_demangle_cpp(ident: &str, opts: DemangleOptions) -> Option<String> {
    if is_maybe_msvc(ident) {
        return try_demangle_msvc(ident, opts);
    }

    #[cfg(feature = "cpp")]
    {
        use cpp_demangle::{DemangleOptions as CppOptions, Symbol as CppSymbol};

        // If ident has a suffix of $ followed by 32 hex digits, discard it.
        let ident = {
            let n = ident.len();
            if n < 33 {
                ident
            } else {
                let (front, back) = ident.split_at(n - 33);
                if back.starts_with('$') && back[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                    front
                } else {
                    ident
                }
            }
        };

        let symbol = match CppSymbol::new(ident) {
            Ok(symbol) => symbol,
            Err(_) => return None,
        };

        let mut cpp_options = CppOptions::new();
        if !opts.parameters {
            cpp_options = cpp_options.no_params();
        }
        if !opts.return_type {
            cpp_options = cpp_options.no_return_type();
        }

        match symbol.demangle(&cpp_options) {
            Ok(demangled) => Some(demangled),
            Err(_) => None,
        }
    }
    #[cfg(not(feature = "cpp"))]
    {
        None
    }
}

#[cfg(feature = "rust")]
fn try_demangle_rust(ident: &str, _opts: DemangleOptions) -> Option<String> {
    match rustc_demangle::try_demangle(ident) {
        Ok(demangled) => Some(format!("{:#}", demangled)),
        Err(_) => None,
    }
}

#[cfg(not(feature = "rust"))]
fn try_demangle_rust(_ident: &str, _opts: DemangleOptions) -> Option<String> {
    None
}

#[cfg(feature = "swift")]
fn try_demangle_swift(ident: &str, opts: DemangleOptions) -> Option<String> {
    let mut buf = vec![0; 4096];
    let sym = match CString::new(ident) {
        Ok(sym) => sym,
        Err(_) => return None,
    };

    let mut features = 0;
    if opts.return_type {
        features |= SYMBOLIC_SWIFT_FEATURE_RETURN_TYPE;
    }
    if opts.parameters {
        features |= SYMBOLIC_SWIFT_FEATURE_PARAMETERS;
    }

    unsafe {
        match symbolic_demangle_swift(sym.as_ptr(), buf.as_mut_ptr(), buf.len(), features) {
            0 => None,
            _ => Some(CStr::from_ptr(buf.as_ptr()).to_string_lossy().to_string()),
        }
    }
}

#[cfg(not(feature = "swift"))]
fn try_demangle_swift(_ident: &str, _opts: DemangleOptions) -> Option<String> {
    None
}

fn demangle_objc(ident: &str, _opts: DemangleOptions) -> String {
    ident.to_string()
}

fn try_demangle_objcpp(ident: &str, opts: DemangleOptions) -> Option<String> {
    if is_maybe_objc(ident) {
        Some(demangle_objc(ident, opts))
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
    /// assert_eq!(Name::from("_ZN3foo3barEv").detect_language(), Language::Cpp);
    /// assert_eq!(Name::from("unknown").detect_language(), Language::Unknown);
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
    /// # #[cfg(feature = "cpp")] {
    /// use symbolic_common::Name;
    /// use symbolic_demangle::{Demangle, DemangleOptions};
    ///
    /// assert_eq!(Name::from("_ZN3foo3barEv").demangle(DemangleOptions::name_only()), Some("foo::bar".to_string()));
    /// assert_eq!(Name::from("unknown").demangle(DemangleOptions::name_only()), None);
    /// # }
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
    /// # #[cfg(feature = "cpp")] {
    /// use symbolic_common::Name;
    /// use symbolic_demangle::{Demangle, DemangleOptions};
    ///
    /// assert_eq!(Name::from("_ZN3foo3barEv").try_demangle(DemangleOptions::name_only()), "foo::bar");
    /// assert_eq!(Name::from("unknown").try_demangle(DemangleOptions::name_only()), "unknown");
    /// # }
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

        #[cfg(feature = "rust")]
        {
            if rustc_demangle::try_demangle(self.as_str()).is_ok() {
                return Language::Rust;
            }
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
        if matches!(self.mangling(), NameMangling::Unmangled) {
            return Some(self.to_string());
        }
        match self.detect_language() {
            Language::ObjC => Some(demangle_objc(self.as_str(), opts)),
            Language::ObjCpp => try_demangle_objcpp(self.as_str(), opts),
            Language::Rust => try_demangle_rust(self.as_str(), opts),
            Language::Cpp => try_demangle_cpp(self.as_str(), opts),
            Language::Swift => try_demangle_swift(self.as_str(), opts),
            _ => None,
        }
    }

    fn try_demangle(&self, opts: DemangleOptions) -> Cow<'_, str> {
        if matches!(self.mangling(), NameMangling::Unmangled) {
            return Cow::Borrowed(self.as_str());
        }
        match self.demangle(opts) {
            Some(demangled) => Cow::Owned(demangled),
            None => Cow::Borrowed(self.as_str()),
        }
    }
}

/// Demangles an identifier and falls back to the original symbol.
///
/// This is a shortcut for [`Demangle::try_demangle`] with complete demangling.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "cpp")] {
/// assert_eq!(symbolic_demangle::demangle("_ZN3foo3barEv"), "foo::bar()");
/// # }
/// ```
///
/// [`Demangle::try_demangle`]: trait.Demangle.html#tymethod.try_demangle
pub fn demangle(ident: &str) -> Cow<'_, str> {
    match Name::from(ident).demangle(DemangleOptions::complete()) {
        Some(demangled) => Cow::Owned(demangled),
        None => Cow::Borrowed(ident),
    }
}
