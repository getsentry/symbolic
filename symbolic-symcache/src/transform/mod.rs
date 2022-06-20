//! Utilities that transform the Data to be written to a SymCache.

mod bcsymbolmap;
pub use bcsymbolmap::*;

#[cfg(feature = "il2cpp")]
pub mod il2cpp;

use std::borrow::Cow;

/// A Function record to be written to the SymCache.
#[non_exhaustive]
pub struct Function<'s> {
    /// The functions name.
    pub name: Cow<'s, str>,
    /// The compilation directory of the function.
    pub comp_dir: Option<Cow<'s, str>>,
}

/// A File to be written to the SymCache.
#[non_exhaustive]
pub struct File<'s> {
    /// The file name.
    pub name: Cow<'s, str>,
    /// The optional directory prefix.
    pub directory: Option<Cow<'s, str>>,
    /// The optional compilation directory prefix.
    pub comp_dir: Option<Cow<'s, str>>,
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
    fn transform_function<'f>(&'f mut self, f: Function<'f>) -> Function<'f> {
        f
    }

    /// Transforms a [`SourceLocation`].
    ///
    /// This can be used for example to apply a Source Mapping in case an intermediate compilation
    /// step might have introduced an indirection, or to de-obfuscate the [`File`] information.
    fn transform_source_location<'f>(&'f mut self, sl: SourceLocation<'f>) -> SourceLocation<'f> {
        sl
    }
}

// This is essentially just a newtype in order to implement `Debug`.
#[derive(Default)]
pub(crate) struct Transformers<'a>(pub Vec<Box<dyn Transformer + 'a>>);

impl<'a> std::fmt::Debug for Transformers<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.0.len();
        f.debug_tuple("Transformers").field(&len).finish()
    }
}
