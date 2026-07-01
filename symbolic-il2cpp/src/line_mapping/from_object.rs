use std::collections::BTreeMap;
use std::io::Write;
use std::iter::Enumerate;
use std::str::Lines;

use symbolic_common::{ByteView, DebugId};
use symbolic_debuginfo::{DebugSession, ObjectLike};

/// A line mapping extracted from an object.
///
/// This is only intended as an intermediate structure for serialization,
/// not for lookups.
pub struct ObjectLineMapping {
    mapping: BTreeMap<String, BTreeMap<String, BTreeMap<u32, u32>>>,
    debug_id: DebugId,
}

impl ObjectLineMapping {
    /// Create a line mapping from the given `object`.
    ///
    /// The mapping is constructed by iterating over all the source files referenced by `object` and
    /// parsing Il2cpp `source_info` records from each. The referenced C++ source files are read
    /// from the local filesystem.
    pub fn from_object<'data, 'object, O, E>(object: &'object O) -> Result<Self, E>
    where
        O: ObjectLike<'data, 'object, Error = E>,
    {
        // Read the referenced source files from the local filesystem.
        Self::from_object_with_provider(object, |path| ByteView::open(path).ok())
    }

    /// Create a line mapping from the given `object`, obtaining the referenced
    /// C++ source file contents from `provider`.
    ///
    /// This is the filesystem-free counterpart of [`Self::from_object`], for
    /// environments without filesystem access (e.g. WebAssembly): the object's
    /// referenced source paths are enumerated via its debug session, and each is
    /// passed to `provider`, which returns the file's bytes (or `None` to skip
    /// it). Only files containing Il2cpp `source_info` records contribute to the
    /// mapping.
    pub fn from_object_with_provider<'data, 'object, O, E, B, P>(
        object: &'object O,
        mut provider: P,
    ) -> Result<Self, E>
    where
        O: ObjectLike<'data, 'object, Error = E>,
        B: AsRef<[u8]>,
        P: FnMut(&str) -> Option<B>,
    {
        let session = object.debug_session()?;
        let debug_id = object.debug_id();

        let mut mapping = BTreeMap::new();

        for cpp_file in session.files() {
            let cpp_file_path = cpp_file?.abs_path_str();
            if mapping.contains_key(&cpp_file_path) {
                continue;
            }

            if let Some(cpp_source) = provider(&cpp_file_path) {
                let cpp_mapping = Self::parse_source_file(cpp_source.as_ref());
                if !cpp_mapping.is_empty() {
                    mapping.insert(cpp_file_path, cpp_mapping);
                }
            }
        }

        Ok(Self { mapping, debug_id })
    }

    /// Create a line mapping from the source file.
    ///
    /// The mapping is constructed by parsing Il2cpp `source_info` records in the given source file.
    pub(crate) fn parse_source_file(cpp_source: &[u8]) -> BTreeMap<String, BTreeMap<u32, u32>> {
        let mut cpp_mapping = BTreeMap::new();

        for SourceInfo {
            cpp_line,
            cs_file,
            cs_line,
        } in SourceInfos::new(cpp_source)
        {
            let cs_mapping = cpp_mapping
                .entry(cs_file.to_string())
                .or_insert_with(BTreeMap::new);
            cs_mapping.insert(cpp_line, cs_line);
        }

        cpp_mapping
    }

    /// Serializes the line mapping to the given writer as JSON.
    ///
    /// The mapping is serialized in the form of nested objects:
    /// C++ file => C# file => C++ line => C# line
    ///
    /// Returns `false` if the resulting JSON did not contain any mappings.
    pub fn to_writer<W: Write>(mut self, writer: &mut W) -> std::io::Result<bool> {
        let is_empty = self.mapping.is_empty();

        // This is a big hack: We need the files for different architectures to be different.
        // To achieve this, we put the debug-id of the file (which is different between architectures)
        // into the same structure as the normal map, like so:
        // `"__debug-id__": {"00000000-0000-0000-0000-000000000000": {}}`
        // When parsing via `LineMapping::parse`, this *looks like* a valid entry, but we will
        // most likely never have a C++ file named `__debug-id__` ;-)
        let value = BTreeMap::from([(self.debug_id.to_string(), Default::default())]);
        self.mapping.insert("__debug-id__".to_owned(), value);

        serde_json::to_writer(writer, &self.mapping)?;
        Ok(!is_empty)
    }
}

/// An Il2cpp `source_info` record.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SourceInfo<'data> {
    /// The C++ source line the `source_info` was parsed from.
    pub(crate) cpp_line: u32,
    /// The corresponding C# source file.
    cs_file: &'data str,
    /// The corresponding C# source line.
    pub(crate) cs_line: u32,
}

/// An iterator over Il2cpp `source_info` markers.
///
/// The Iterator yields `SourceInfo`s.
pub(crate) struct SourceInfos<'data> {
    lines: Enumerate<Lines<'data>>,
    current: Option<(&'data str, u32)>,
}

impl<'data> SourceInfos<'data> {
    /// Parses the `source` leniently, yielding an empty Iterator for non-utf8 data.
    pub(crate) fn new(source: &'data [u8]) -> Self {
        let lines = std::str::from_utf8(source)
            .ok()
            .unwrap_or_default()
            .lines()
            .enumerate();
        Self {
            lines,
            current: None,
        }
    }
}

impl<'data> Iterator for SourceInfos<'data> {
    type Item = SourceInfo<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        for (cpp_line_nr, cpp_src_line) in &mut self.lines {
            match parse_line(cpp_src_line) {
                // A new source info record. Emit the previously found one, if there is one.
                Some((cs_file, cs_line)) => {
                    if let Some((cs_file, cs_line)) = self.current.replace((cs_file, cs_line)) {
                        return Some(SourceInfo {
                            cpp_line: cpp_line_nr as u32,
                            cs_file,
                            cs_line,
                        });
                    }
                }

                // A comment. Just continue.
                None if cpp_src_line.trim_start().starts_with("//") => continue,
                // A source line. Emit the previously found source info record, if there is one.
                None => {
                    if let Some((cs_file, cs_line)) = self.current.take() {
                        return Some(SourceInfo {
                            cpp_line: (cpp_line_nr + 1) as u32,
                            cs_file,
                            cs_line,
                        });
                    }
                }
            }
        }
        None
    }
}

/// Extracts the `(file, line)` information
///
/// For example, `//<source_info:main.cs:17>`
/// would be parsed as `("main.cs", 17)`.
fn parse_line(line: &str) -> Option<(&str, u32)> {
    let line = line.trim();
    let source_ref = line.strip_prefix("//<source_info:")?;
    let source_ref = source_ref.strip_suffix('>')?;
    let (file, line) = source_ref.rsplit_once(':')?;
    let line = line.parse().ok()?;
    Some((file, line))
}

#[cfg(test)]
mod tests {
    use symbolic_common::ByteView;
    use symbolic_debuginfo::Object;
    use symbolic_testutils::fixture;

    use super::*;

    #[test]
    fn one_mapping() {
        let cpp_source = b"
            Lorem ipsum dolor sit amet
            //<source_info:main.cs:17>
            // some
            // more
            // comments
            actual source code";

        let source_infos: Vec<_> = SourceInfos::new(cpp_source).collect();

        assert_eq!(
            source_infos,
            vec![SourceInfo {
                cpp_line: 7,
                cs_file: "main.cs",
                cs_line: 17,
            }]
        )
    }

    #[test]
    fn several_mappings() {
        let cpp_source = b"
            Lorem ipsum dolor sit amet
            //<source_info:main.cs:17>
            // some
            // comments
            actual source code 1
            actual source code 2

            //<source_info:main.cs:29>
            actual source code 3

            //<source_info:main.cs:46>
            // more
            // comments
            actual source code 4";

        let source_infos: Vec<_> = SourceInfos::new(cpp_source).collect();

        assert_eq!(
            source_infos,
            vec![
                SourceInfo {
                    cpp_line: 6,
                    cs_file: "main.cs",
                    cs_line: 17,
                },
                SourceInfo {
                    cpp_line: 10,
                    cs_file: "main.cs",
                    cs_line: 29,
                },
                SourceInfo {
                    cpp_line: 15,
                    cs_file: "main.cs",
                    cs_line: 46,
                }
            ]
        )
    }

    #[test]
    fn missing_source_line() {
        let cpp_source = b"
            Lorem ipsum dolor sit amet
            //<source_info:main.cs:17>
            // some
            // comments
            //<source_info:main.cs:29>
            actual source code";

        let source_infos: Vec<_> = SourceInfos::new(cpp_source).collect();

        // The first source info has no source line to attach to, so it should use the line
        // immediately before the second source_info.
        assert_eq!(
            source_infos,
            vec![
                SourceInfo {
                    cpp_line: 5,
                    cs_file: "main.cs",
                    cs_line: 17,
                },
                SourceInfo {
                    cpp_line: 7,
                    cs_file: "main.cs",
                    cs_line: 29,
                },
            ]
        )
    }

    #[test]
    fn broken() {
        let cpp_source = b"
            Lorem ipsum dolor sit amet
            //<source_info:main.cs:17>
            // some
            // more
            // comments";

        // Since there is no non-comment line for the source info to attach to,
        // no source infos should be returned.
        assert_eq!(SourceInfos::new(cpp_source).count(), 0);
    }

    /// Synthetic Il2cpp C++: a `source_info` marker followed by a code line maps the
    /// generated C++ line 2 to `Game.cs` line 42.
    const SYNTHETIC_SOURCE: &[u8] = b"//<source_info:Game.cs:42>\nint generated = 0;\n";

    #[test]
    fn test_object_line_mapping_parses_source_info() {
        let data = ByteView::open(fixture("windows/Sentry.Samples.Console.Basic.pdb")).unwrap();
        let object = Object::parse(&data).unwrap();

        let mut calls = 0usize;
        let mapping = ObjectLineMapping::from_object_with_provider(&object, |path| {
            assert!(!path.is_empty());
            calls += 1;
            Some(SYNTHETIC_SOURCE.to_vec())
        })
        .unwrap();

        assert!(calls > 0);

        let mut buf = Vec::new();
        assert!(mapping.to_writer(&mut buf).unwrap());

        let json: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        insta::assert_json_snapshot!(json, @r#"
        {
          "C:\\dev\\sentry-dotnet\\samples\\Sentry.Samples.Console.Basic\\Program.cs": {
            "Game.cs": {
              "2": 42
            }
          },
          "C:\\dev\\sentry-dotnet\\samples\\Sentry.Samples.Console.Basic\\obj\\release\\net6.0\\.NETCoreApp,Version=v6.0.AssemblyAttributes.cs": {
            "Game.cs": {
              "2": 42
            }
          },
          "C:\\dev\\sentry-dotnet\\samples\\Sentry.Samples.Console.Basic\\obj\\release\\net6.0\\Sentry.Samples.Console.Basic.AssemblyInfo.cs": {
            "Game.cs": {
              "2": 42
            }
          },
          "C:\\dev\\sentry-dotnet\\samples\\Sentry.Samples.Console.Basic\\obj\\release\\net6.0\\Sentry.Samples.Console.Basic.GlobalUsings.g.cs": {
            "Game.cs": {
              "2": 42
            }
          },
          "__debug-id__": {
            "526f365f-4d8d-4fa8-b370-eae9a9136de4-a39453e5": {}
          }
        }
        "#);
    }

    #[test]
    fn test_object_line_mapping_no_sources() {
        let view = ByteView::open(fixture("windows/Sentry.Samples.Console.Basic.pdb")).unwrap();
        let object = Object::parse(&view).unwrap();

        let mapping =
            ObjectLineMapping::from_object_with_provider(&object, |_| None::<Vec<u8>>).unwrap();

        let mut buf = Vec::new();
        assert!(!mapping.to_writer(&mut buf).unwrap());
    }
}
