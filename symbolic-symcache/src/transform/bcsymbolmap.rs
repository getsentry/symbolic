//! Resolves obfuscated symbols against a BCSymbolMap before writing them to a SymCache.

use std::borrow::Cow;

use symbolic_debuginfo::macho::BcSymbolMap;

use super::{File, Function, SourceLocation, Transformer};

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
