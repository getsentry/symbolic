mod from_object;

use indexmap::IndexSet;
use std::collections::HashMap;

pub use from_object::ObjectLineMapping;

/// An internal line mapping.
#[derive(Debug)]
struct LineEntry {
    /// The C++ line that is being mapped.
    cpp_line: u32,
    /// The C# line it corresponds to.
    cs_line: u32,
    /// The index into the `cs_files` [`IndexSet`] below for the corresponding C# file.
    cs_file_idx: usize,
}

/// A parsed Il2Cpp/Unity Line mapping JSON.
#[derive(Debug, Default)]
pub struct LineMapping {
    /// The set of C# files.
    cs_files: IndexSet<String>,
    /// A map of C++ filename to a list of Mappings.
    cpp_file_map: HashMap<String, Vec<LineEntry>>,
}

impl LineMapping {
    /// Parses a JSON buffer into a valid [`LineMapping`].
    ///
    /// Returns [`None`] if the JSON was not a valid mapping.
    pub fn parse(data: &[u8]) -> Option<Self> {
        let json: serde_json::Value = serde_json::from_slice(data).ok()?;
        let mut result = Self::default();

        if let serde_json::Value::Object(object) = json {
            for (cpp_file, file_map) in object {
                // This is a sentinel value for the originating debug file, which
                // `ObjectLineMapping::to_writer` writes to the file to make it unique
                // (and dependent on the originating debug-id).
                if cpp_file == "__debug-id__" {
                    continue;
                }
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

    /// Looks up the corresponding C# file/line for a given C++ file/line.
    ///
    /// As these mappings are not exact, this will return an exact match, or a mapping "close-by".
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
            cpp_line,
        } = lines.get(idx)?;

        // We will return mappings at most 5 lines away from the source line they refer to.
        if line.saturating_sub(*cpp_line) > 5 {
            return None;
        }

        Some((self.cs_files.get_index(*cs_file_idx)?, *cs_line))
    }
}

#[cfg(test)]
mod tests {
    use super::from_object::SourceInfos;
    use super::*;

    #[test]
    fn test_lookup() {
        // well, we can either use a pre-made json, or create one ourselves:
        let cpp_source = b"Lorem ipsum dolor sit amet
            //<source_info:main.cs:17>
            // some
            // comments
            some expression // 5
            stretching
            over
            multiple lines

            // blank lines

            // and stuff
            // 13
            //<source_info:main.cs:29>
            actual source code // 15
        ";

        let line_mappings: HashMap<_, _> = SourceInfos::new(cpp_source)
            .map(|si| (si.cpp_line, si.cs_line))
            .collect();

        let mapping = HashMap::from([("main.cpp", HashMap::from([("main.cs", line_mappings)]))]);
        let mapping_json = serde_json::to_string(&mapping).unwrap();

        let parsed_mapping = LineMapping::parse(mapping_json.as_bytes()).unwrap();

        assert_eq!(parsed_mapping.lookup("main.cpp", 2), None);
        assert_eq!(parsed_mapping.lookup("main.cpp", 5), Some(("main.cs", 17)));
        assert_eq!(parsed_mapping.lookup("main.cpp", 12), None);
        assert_eq!(parsed_mapping.lookup("main.cpp", 15), Some(("main.cs", 29)));
    }
}
