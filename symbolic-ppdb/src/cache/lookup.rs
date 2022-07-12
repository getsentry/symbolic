use symbolic_common::Language;

use super::PortablePdbCache;

/// Line information for a given IL offset.
#[derive(Debug, Clone)]
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
    pub fn lookup(&self, method: usize, il_offset: usize) -> Option<LineInfo<'data>> {
        let sl = match self
            .ranges
            .binary_search_by_key(&(method, il_offset), |range| {
                (range.idx as usize, range.il_offset as usize)
            }) {
            Ok(idx) => self.source_locations.get(idx)?,
            Err(idx) => {
                let idx = idx.checked_sub(1)?;
                let range = self.ranges.get(idx)?;
                if (range.idx as usize) < method {
                    return None;
                }

                self.source_locations.get(idx)?
            }
        };

        let file_name = self.get_string(sl.file_name_idx)?;

        Some(LineInfo {
            line: sl.line,
            file_name,
            file_lang: Language::from_u32(sl.lang),
        })
    }

    /// Resolves a string reference to the pointed-to `&str` data.
    fn get_string(&self, offset: u32) -> Option<&'data str> {
        let reader = &mut self.string_bytes.get(offset as usize..)?;
        let len = leb128::read::unsigned(reader).ok()? as usize;

        let bytes = reader.get(..len)?;

        std::str::from_utf8(bytes).ok()
    }
}
