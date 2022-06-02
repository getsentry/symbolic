use std::iter::Enumerate;
use std::str::Lines;
use std::{collections::BTreeMap, io::Write};

use symbolic_common::ByteView;
use symbolic_debuginfo::{DebugSession, ObjectLike};

/// A line mapping extracted from an object.
///
/// This is only intended as an intermediate structure for serialization,
/// not for lookups.
pub struct ObjectLineMapping(BTreeMap<String, BTreeMap<String, BTreeMap<u32, u32>>>);

impl ObjectLineMapping {
    /// Create a line mapping from the given `object`.
    ///
    /// The mapping is constructed by iterating over all the source files referenced by `object` and
    /// parsing Il2cpp `source_info` records from each.
    pub fn from_object<'data, 'object, O, E>(object: &'object O) -> Result<Self, E>
    where
        O: ObjectLike<'data, 'object, Error = E>,
    {
        let session = object.debug_session()?;

        let mut mapping = BTreeMap::new();

        for cpp_file in session.files() {
            let cpp_file_path = cpp_file?.abs_path_str();
            if mapping.contains_key(&cpp_file_path) {
                continue;
            }

            if let Ok(cpp_source) = ByteView::open(&cpp_file_path) {
                let mut cpp_mapping = BTreeMap::new();

                for SourceInfo {
                    cpp_line,
                    cs_file,
                    cs_line,
                } in SourceInfos::new(&cpp_source)
                {
                    let cs_mapping = cpp_mapping
                        .entry(cs_file.to_string())
                        .or_insert_with(BTreeMap::new);
                    cs_mapping.insert(cpp_line, cs_line);
                }

                if !cpp_mapping.is_empty() {
                    mapping.insert(cpp_file_path, cpp_mapping);
                }
            }
        }

        Ok(Self(mapping))
    }

    /// Serializes the line mapping to the given writer as JSON.
    ///
    /// The mapping is serialized in the form of nested objects:
    /// C++ file => C# file => C++ line => C# line
    pub fn to_writer<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        serde_json::to_writer(writer, &self.0)?;
        Ok(())
    }
}

/// An Il2cpp `source_info` record.
struct SourceInfo<'data> {
    /// The C++ source line the `source_info` was parsed from.
    cpp_line: u32,
    /// The corresponding C# source file.
    cs_file: &'data str,
    /// The corresponding C# source line.
    cs_line: u32,
}

/// An iterator over Il2cpp `source_info` markers.
///
/// The Iterator yields `(file, line)` pairs.
struct SourceInfos<'data> {
    lines: Enumerate<Lines<'data>>,
}

impl<'data> SourceInfos<'data> {
    /// Parses the `source` leniently, yielding an empty Iterator for non-utf8 data.
    fn new(source: &'data [u8]) -> Self {
        let lines = std::str::from_utf8(source)
            .ok()
            .unwrap_or_default()
            .lines()
            .enumerate();
        Self { lines }
    }
}

impl<'data> Iterator for SourceInfos<'data> {
    type Item = SourceInfo<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        for (cpp_line, cpp_src_line) in &mut self.lines {
            match parse_line(cpp_src_line) {
                Some((cs_file, cs_line)) => {
                    return Some(SourceInfo {
                        cpp_line: (cpp_line + 1) as u32,
                        cs_file,
                        cs_line,
                    })
                }
                None => continue,
            }
        }
        None
    }
}

/// Extracts the `(file, line)` information
fn parse_line(line: &str) -> Option<(&str, u32)> {
    let line = line.trim();
    let source_ref = line.strip_prefix("//<source_info:")?;
    let source_ref = source_ref.strip_suffix('>')?;
    let (file, line) = source_ref.rsplit_once(':')?;
    let line = line.parse().ok()?;
    Some((file, line))
}
