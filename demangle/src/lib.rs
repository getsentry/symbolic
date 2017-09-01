//! Provides demangling support.
//!
//! Currently supported languages:
//!
//! * C++
//! * Rust
extern crate symbolic_common;
extern crate rustc_demangle;
extern crate cpp_demangle;

use symbolic_common::{ErrorKind, Result};

/// Supported programming languages for demangling
#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum Language {
    Cpp,
    Rust,
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
    /// Should arguments be returned?
    pub with_arguments: bool,
    /// languages that should be attempted for demangling
    ///
    /// These languages are tried in the order defined.  This is releavant
    /// as some mangling schemes overlap for some trivial cases (for
    /// instance rust and C++).
    pub languages: Vec<Language>,
}

impl Default for DemangleOptions {
    fn default() -> DemangleOptions {
        DemangleOptions {
            format: DemangleFormat::Short,
            with_arguments: false,
            languages: vec![Language::Cpp, Language::Rust],
        }
    }
}

fn try_demangle_cpp(ident: &str, opts: &DemangleOptions) -> Result<Option<String>> {
    if &ident[..2] != "_Z" {
        return Ok(None);
    }
    match cpp_demangle::Symbol::new(ident) {
        Ok(sym) => {
            Ok(sym.demangle(&cpp_demangle::DemangleOptions {
                no_params: !opts.with_arguments
            }).ok())
        }
        Err(err) => {
            Err(ErrorKind::BadSymbol(err.to_string()).into())
        }
    }
}

fn try_demangle_rust(ident: &str, _opts: &DemangleOptions) -> Result<Option<String>> {
    if let Ok(dm) = rustc_demangle::try_demangle(ident) {
        Ok(Some(format!("{:#}", dm)))
    } else {
        Ok(None)
    }
}

/// Demangles an identifier.
///
/// Example:
///
/// ```
/// # use symbolic_demangle::*;
/// let rv = demangle("_ZN3foo3barE", &Default::default()).unwrap();
/// assert_eq!(&rv.unwrap(), "foo::bar");
/// ```
pub fn demangle(ident: &str, opts: &DemangleOptions) -> Result<Option<String>> {
    for &lang in &opts.languages {
        if let Some(rv) = match lang {
            Language::Cpp => try_demangle_cpp(ident, opts)?,
            Language::Rust => try_demangle_rust(ident, opts)?,
        } {
            return Ok(Some(rv));
        }
    }
    Ok(None)
}
