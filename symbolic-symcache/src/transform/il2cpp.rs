//! Resolves IL2CPP-compiled native symbols into their managed equivalents using a mapping file
//! before writing them to a SymCache.

use symbolic_il2cpp::LineMapping;

use super::{File, Function, SourceLocation, Transformer};

fn full_path(file: &File<'_>) -> String {
    let comp_dir = file.comp_dir.as_deref().unwrap_or_default();
    let directory = file.directory.as_deref().unwrap_or_default();
    let path_name = &file.name;

    let prefix = symbolic_common::join_path(comp_dir, directory);
    let full_path = symbolic_common::join_path(&prefix, path_name);
    symbolic_common::clean_path(&full_path).into_owned()
}

impl Transformer for LineMapping {
    fn transform_function<'f>(&'f mut self, f: Function<'f>) -> Function<'f> {
        f
    }

    fn transform_source_location<'f>(
        &'f mut self,
        mut sl: SourceLocation<'f>,
    ) -> SourceLocation<'f> {
        // TODO: this allocates, which is especially expensive since we run this transformer for
        // every single source location (without dedupe-ing files). It might be worth caching this
        let full_path = full_path(&sl.file);
        if let Some((mapped_file, mapped_line)) = self.lookup(&full_path, sl.line) {
            sl.file.name = mapped_file.into();
            sl.file.comp_dir = None;
            sl.file.directory = None;
            sl.line = mapped_line;
        }

        sl
    }
}
