use std::iter::Enumerate;
use std::str::Lines;

/// An Il2cpp `source_info` record.
#[non_exhaustive]
pub struct SourceInfo<'data> {
    /// The C++ source line the `source_info` was parsed from.
    pub cpp_line: u32,
    /// The corresponding C# source file.
    pub cs_file: &'data str,
    /// The corresponding C# source line.
    pub cs_line: u32,
}

/// An iterator over Il2cpp `source_info` markers.
///
/// The Iterator yields `(file, line)` pairs.
pub struct SourceInfoParser<'data> {
    lines: Enumerate<Lines<'data>>,
}

impl<'data> SourceInfoParser<'data> {
    /// Parses the `source` leniently, yielding an empty Iterator for non-utf8 data.
    pub fn new(source: &'data [u8]) -> Self {
        let lines = std::str::from_utf8(source)
            .ok()
            .unwrap_or_default()
            .lines()
            .enumerate();
        Self { lines }
    }
}

impl<'data> Iterator for SourceInfoParser<'data> {
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
