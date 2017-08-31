//! Provides demangling support.
extern crate symbolic_common;
extern crate rustc_demangle;
extern crate cpp_demangle;

use symbolic_common::{ErrorKind, Result};

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum Language {
    Cpp,
    Swift,
    Rust,
}

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum DemangleFormat {
    Short,
    Full,
}

#[derive(Debug, Clone)]
pub struct DemangleOptions {
    pub format: DemangleFormat,
    pub languages: Vec<Language>,
}

impl Default for DemangleOptions {
    fn default() -> DemangleOptions {
        DemangleOptions {
            format: DemangleFormat::Short,
            languages: vec![Language::Cpp, Language::Swift, Language::Rust],
        }
    }
}

/// Demangles an identifier.
pub fn demangle(ident: &str, opts: &DemangleOptions) -> Result<Option<String>> {
    for &lang in &opts.languages {
        match lang {
            Language::Cpp => {
                match cpp_demangle::Symbol::new(ident) {
                    Ok(sym) => {
                        return Ok(sym.demangle(&match opts.format {
                            DemangleFormat::Short => {
                                cpp_demangle::DemangleOptions {
                                    no_params: true,
                                }
                            },
                            DemangleFormat::Full => {
                                cpp_demangle::DemangleOptions {
                                    no_params: false,
                                }
                            }
                        }).ok())
                    }
                    Err(err) => {
                        return Err(ErrorKind::BadSymbol(err.to_string()).into());
                    }
                }
            },
            Language::Swift => {},
            Language::Rust => {
                if let Ok(dm) = rustc_demangle::try_demangle(ident) {
                    return Ok(Some(format!("{:#}", dm)));
                }
            }
        }
    }
    Ok(None)
}
