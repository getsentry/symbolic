use indexmap::IndexSet;
use std::collections::HashMap;

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
