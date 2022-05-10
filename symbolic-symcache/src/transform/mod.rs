//! Utilities that transform the Data to be written to a SymCache.

mod bcsymbolmap;
pub use bcsymbolmap::*;
use camino::{Utf8Path, Utf8PathBuf};

#[cfg(feature = "il2cpp")]
pub mod il2cpp;

use std::borrow::Cow;

use crate::normalize_path;

/// A Function record to be written to the SymCache.
#[non_exhaustive]
pub struct Function<'s> {
    /// The functions name.
    pub name: Cow<'s, str>,
    /// The compilation directory of the function.
    pub comp_dir: Option<Cow<'s, Utf8Path>>,
}

/// A File to be written to the SymCache.
#[non_exhaustive]
pub struct File<'s> {
    /// The file name.
    pub name: Cow<'s, Utf8Path>,
    /// The optional directory prefix.
    pub directory: Option<Cow<'s, Utf8Path>>,
    /// The optional compilation directory prefix.
    pub comp_dir: Option<Cow<'s, Utf8Path>>,
}

impl File<'_> {
    fn full_path(&self) -> Utf8PathBuf {
        let mut path = Utf8PathBuf::new();
        if let Some(ref comp_dir) = self.comp_dir {
            normalize_path(&mut path, comp_dir);
        }
        if let Some(ref dir) = self.directory {
            normalize_path(&mut path, dir);
        }
        normalize_path(&mut path, self.name.as_ref());
        path
    }
}

/// A Source Location (File + Line) to be written to the SymCache.
#[non_exhaustive]
pub struct SourceLocation<'s> {
    /// The [`File`] part of this [`SourceLocation`].
    pub file: File<'s>,
    /// The line number.
    pub line: u32,
}

/// A transformer that is applied to each [`Function`] and [`SourceLocation`] record in the SymCache.
pub trait Transformer {
    /// Transforms a [`Function`] record.
    ///
    /// This can be used for example to de-obfuscate a functions name.
    fn transform_function<'f>(&'f self, f: Function<'f>) -> Function<'f> {
        f
    }

    /// Transforms a [`SourceLocation`].
    ///
    /// This can be used for example to apply a Source Mapping in case an intermediate compilation
    /// step might have introduced an indirection, or to de-obfuscate the [`File`] information.
    fn transform_source_location<'f>(&'f self, sl: SourceLocation<'f>) -> SourceLocation<'f> {
        sl
    }
}

// This is essentially just a newtype in order to implement `Debug`.
#[derive(Default)]
pub(crate) struct Transformers(pub Vec<Box<dyn Transformer>>);

impl std::fmt::Debug for Transformers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.0.len();
        f.debug_tuple("Transformers").field(&len).finish()
    }
}
