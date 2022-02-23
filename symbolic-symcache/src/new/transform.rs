//! Utilities that transform the Data to be written to a SymCache.

use std::borrow::Cow;

use symbolic_debuginfo::macho::BcSymbolMap;

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
pub(crate) struct Transformers(pub Vec<Box<dyn Transformer>>);

impl std::fmt::Debug for Transformers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.0.len();
        f.debug_tuple("Transformers").field(&len).finish()
    }
}

// This ended up as a macro which "inlines" mapping the `Cow` into the calling function, as using
// a real function here would lead to the following borrow checker error:
// error[E0495]: cannot infer an appropriate lifetime for lifetime parameter `'d` due to conflicting requirements
macro_rules! map_cow {
    ($cow:expr, $f: expr) => {
        match $cow {
            Cow::Borrowed(inner) => Cow::Borrowed($f(inner)),
            Cow::Owned(inner) => Cow::Owned($f(&inner).to_owned()),
        }
    };
}

impl Transformer for BcSymbolMap<'_> {
    fn transform_function<'f>(&'f mut self, f: Function<'f>) -> Function<'f> {
        Function {
            name: map_cow!(f.name, |s| self.resolve(s)),
            comp_dir: f.comp_dir.map(|dir| map_cow!(dir, |s| self.resolve(s))),
        }
    }

    fn transform_source_location<'f>(&'f mut self, sl: SourceLocation<'f>) -> SourceLocation<'f> {
        SourceLocation {
            file: File {
                name: map_cow!(sl.file.name, |s| self.resolve(s)),
                directory: sl
                    .file
                    .directory
                    .map(|dir| map_cow!(dir, |s| self.resolve(s))),
                comp_dir: sl
                    .file
                    .comp_dir
                    .map(|dir| map_cow!(dir, |s| self.resolve(s))),
            },
            line: sl.line,
        }
    }
}
