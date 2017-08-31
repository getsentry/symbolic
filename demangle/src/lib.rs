//! Provides demangling support.
extern crate symbolic_common;

use symbolic_common::Result;

#[derive(Eq, PartialEq, Debug)]
pub enum Language {
    Cpp,
    Swift,
}

#[derive(Eq, PartialEq, Debug)]
pub enum DemangleFormat {
    Short,
    Full,
}

#[derive(Debug)]
pub struct DemangleOptions {
    format: DemangleFormat,
    languages: Vec<Language>,
}

impl Default for DemangleOptions {
    fn default() -> DemangleOptions {
        DemangleOptions {
            format: DemangleFormat::Short,
            languages: vec![Language::Cpp, Language::Swift],
        }
    }
}

/// Demangles an identifier.
pub fn demangle(_ident: &str, _opts: DemangleOptions) -> Result<Option<String>> {
    Ok(None)
}
