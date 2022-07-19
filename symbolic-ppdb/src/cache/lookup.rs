use symbolic_common::Language;

use super::{raw, PortablePdbCache};

/// Line information for a given IL offset.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LineInfo<'data> {
    /// The line in the source file.
    pub line: u32,
    /// The source file's name.
    pub file_name: &'data str,
    /// The source language.
    pub file_lang: Language,
}

impl<'data> PortablePdbCache<'data> {
    /// Looks up line information for the given IL offset for the method with the given index.
    ///
    /// Note that the method index is 1-based!
    pub fn lookup(&self, function: u32, il_offset: u32) -> Option<LineInfo<'data>> {
        let range = raw::Range {
            func_idx: function,
            il_offset,
        };
        let sl = match self.ranges.binary_search(&range) {
            Ok(idx) => self.source_locations.get(idx)?,
            Err(idx) => {
                let idx = idx.checked_sub(1)?;
                let range = self.ranges.get(idx)?;
                if range.func_idx < function {
                    return None;
                }

                self.source_locations.get(idx)?
            }
        };

        let (file_name, file_lang) = self.get_file(sl.file_idx)?;

        Some(LineInfo {
            line: sl.line,
            file_name,
            file_lang,
        })
    }

    fn get_file(&self, idx: u32) -> Option<(&'data str, Language)> {
        let raw = self.files.get(idx as usize)?;
        let name = self.get_string(raw.name_offset)?;

        Some((name, Language::from_u32(raw.lang)))
    }

    /// Resolves a string reference to the pointed-to `&str` data.
    fn get_string(&self, offset: u32) -> Option<&'data str> {
        let reader = &mut self.string_bytes.get(offset as usize..)?;
        let len = leb128::read::unsigned(reader).ok()? as usize;

        let bytes = reader.get(..len)?;

        std::str::from_utf8(bytes).ok()
    }
}
