use indexmap::IndexSet;
use std::collections::HashMap;
use symbolic_symcache::transform::{self, Transformer};

#[derive(Debug)]
struct LineEntry {
    cpp_line: u32,
    cs_line: u32,
    cs_file_idx: usize,
}

#[derive(Debug, Default)]
pub struct LineMapping {
    cs_files: IndexSet<String>,
    cpp_file_map: HashMap<String, Vec<LineEntry>>,
}

impl LineMapping {
    pub fn parse(data: &[u8]) -> Option<Self> {
        let json: serde_json::Value = serde_json::from_slice(data).ok()?;
        let mut result = Self::default();

        if let serde_json::Value::Object(object) = json {
            for (cpp_file, file_map) in object {
                let mut lines = Vec::new();
                if let serde_json::Value::Object(file_map) = file_map {
                    for (cs_file, line_map) in file_map {
                        if let serde_json::Value::Object(line_map) = line_map {
                            let cs_file_idx = result.cs_files.insert_full(cs_file).0;
                            for (from, to) in line_map {
                                let cpp_line = from.parse().ok()?;
                                let cs_line = to.as_u64().and_then(|n| n.try_into().ok())?;
                                lines.push(LineEntry {
                                    cpp_line,
                                    cs_line,
                                    cs_file_idx,
                                });
                            }
                        }
                    }
                }
                lines.sort_by_key(|entry| entry.cpp_line);
                result.cpp_file_map.insert(cpp_file, lines);
            }
        }

        Some(result)
    }

    pub fn lookup(&self, file: &str, line: u32) -> Option<(&str, u32)> {
        let lines = self.cpp_file_map.get(file)?;

        let idx = match lines.binary_search_by_key(&line, |entry| entry.cpp_line) {
            Ok(idx) => idx,
            Err(0) => return None,
            Err(idx) => idx - 1,
        };

        let LineEntry {
            cs_line,
            cs_file_idx,
            ..
        } = lines.get(idx)?;

        Some((self.cs_files.get_index(*cs_file_idx)?, *cs_line))
    }
}

fn full_path(file: &transform::File<'_>) -> String {
    let comp_dir = file.comp_dir.as_deref().unwrap_or_default();
    let directory = file.directory.as_deref().unwrap_or_default();
    let path_name = &file.name;

    let prefix = symbolic_common::join_path(comp_dir, directory);
    let full_path = symbolic_common::join_path(&prefix, path_name);
    symbolic_common::clean_path(&full_path).into_owned()
}

impl Transformer for LineMapping {
    fn transform_function<'f>(&'f self, f: transform::Function<'f>) -> transform::Function<'f> {
        f
    }

    fn transform_source_location<'f>(
        &'f self,
        mut sl: transform::SourceLocation<'f>,
    ) -> transform::SourceLocation<'f> {
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
