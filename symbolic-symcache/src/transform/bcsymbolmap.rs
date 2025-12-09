//! Resolves obfuscated symbols against a BCSymbolMap before writing them to a SymCache.

use std::borrow::Cow;

use symbolic_debuginfo::macho::BcSymbolMap;

use super::{File, Function, SourceLocation, Transformer};

fn resolve_cow<'f>(map: &'f BcSymbolMap<'_>, s: Cow<'f, str>) -> Cow<'f, str> {
    match s {
        Cow::Borrowed(inner) => Cow::Borrowed(map.resolve(inner)),
        Cow::Owned(inner) => Cow::Owned(map.resolve(&inner).to_owned()),
    }
}

impl Transformer for BcSymbolMap<'_> {
    fn transform_function<'f>(&'f mut self, f: Function<'f>) -> Function<'f> {
        Function {
            name: resolve_cow(self, f.name),
            comp_dir: f.comp_dir.map(|dir| resolve_cow(self, dir)),
        }
    }

    fn transform_source_location<'f>(&'f mut self, sl: SourceLocation<'f>) -> SourceLocation<'f> {
        SourceLocation {
            file: File {
                name: resolve_cow(self, sl.file.name),
                directory: sl.file.directory.map(|dir| resolve_cow(self, dir)),
                comp_dir: sl.file.comp_dir.map(|dir| resolve_cow(self, dir)),
                revision: sl.file.revision,
            },
            line: sl.line,
        }
    }
}
