//! Resolves IL2CPP-compiled native symbols into their managed equivalents using a mapping file
//! before writing them to a SymCache.

use camino::Utf8Path;
use symbolic_il2cpp::LineMapping;

use super::{Function, SourceLocation, Transformer};

impl Transformer for LineMapping {
    fn transform_function<'f>(&'f self, f: Function<'f>) -> Function<'f> {
        f
    }

    fn transform_source_location<'f>(&'f self, mut sl: SourceLocation<'f>) -> SourceLocation<'f> {
        // TODO: this allocates, which is especially expensive since we run this transformer for
        // every single source location (without dedupe-ing files). It might be worth caching this
        let full_path = sl.file.full_path();
        if let Some((mapped_file, mapped_line)) = self.lookup(full_path.as_str(), sl.line) {
            sl.file.name = <&Utf8Path>::from(mapped_file).into();
            sl.file.comp_dir = None;
            sl.file.directory = None;
            sl.line = mapped_line;
        }

        sl
    }
}
