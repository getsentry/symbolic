use std::collections::BTreeMap;

use symbolic_common::ByteView;
use symbolic_debuginfo::{DebugSession, ObjectLike};

use crate::source_info::{SourceInfo, SourceInfoParser};

/// The Line Mapping is represented as nested maps like this:
/// C++ file => C# file => C++ line => C# line
pub type LineMapping = BTreeMap<String, BTreeMap<String, BTreeMap<u32, u32>>>;

/// Create a Line Mapping from the given `object`.
///
/// The mapping is constructed by iterating over all the source files referenced by `object` and
/// parsing Il2cpp `source_info` records from each.
pub fn create_line_mapping_from_object<'data, 'object, O, E>(
    object: &'object O,
) -> Result<LineMapping, E>
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
            } in SourceInfoParser::new(&cpp_source)
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

    Ok(mapping)
}
