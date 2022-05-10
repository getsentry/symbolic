//! Resolves obfuscated symbols against a BCSymbolMap before writing them to a SymCache.

use std::borrow::Cow;

use camino::Utf8Path;
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

fn resolve_utf8path<'d>(map: &BcSymbolMap<'d>, path: &'d Utf8Path) -> &'d Utf8Path {
    map.resolve(path.as_str()).into()
}

impl Transformer for BcSymbolMap<'_> {
    fn transform_function<'f>(&'f self, f: Function<'f>) -> Function<'f> {
        Function {
            name: map_cow!(f.name, |s| self.resolve(s)),
            comp_dir: f
                .comp_dir
                .map(|dir| map_cow!(dir, |s| resolve_utf8path(self, s))),
        }
    }

    fn transform_source_location<'f>(&'f self, sl: SourceLocation<'f>) -> SourceLocation<'f> {
        SourceLocation {
            file: File {
                name: map_cow!(sl.file.name, |s| resolve_utf8path(self, s)),
                directory: sl
                    .file
                    .directory
                    .map(|dir| map_cow!(dir, |s| resolve_utf8path(self, s))),
                comp_dir: sl
                    .file
                    .comp_dir
                    .map(|dir| map_cow!(dir, |s| resolve_utf8path(self, s))),
            },
            line: sl.line,
        }
    }
}
