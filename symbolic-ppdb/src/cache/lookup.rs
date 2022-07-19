use symbolic_common::Language;

use super::{raw, PortablePdbCache};

/// Line information for a given IL offset in a function.
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
    /// Looks up line information for a function in the cache.
    ///
    /// `func_idx` is the (1-based) index of the function in the ECMA-335 `MethodDef` table
    /// (see the ECMA-335 spec, Section II.22.26). In C#, it is encoded in the
    /// [`MetadataToken`](https://docs.microsoft.com/en-us/dotnet/api/system.reflection.memberinfo.metadatatoken?view=net-6.0#system-reflection-memberinfo-metadatatoken)
    /// property on the [`MethodBase`](https://docs.microsoft.com/en-us/dotnet/api/system.reflection.methodbase?view=net-6.0) class.
    /// See [Metadata Tokens](https://docs.microsoft.com/en-us/previous-versions/dotnet/netframework-4.0/ms404456(v=vs.100)) for an
    /// explanation of the encoding.
    ///
    /// `il_offset` is the offset from the start of the method's Intermediate Language code.
    /// It can be obtained via the [`StackFrame.GetILOffset`](https://docs.microsoft.com/en-us/dotnet/api/system.diagnostics.stackframe.getiloffset?view=net-6.0#system-diagnostics-stackframe-getiloffset)
    /// method.
    pub fn lookup(&self, func_idx: u32, il_offset: u32) -> Option<LineInfo<'data>> {
        let range = raw::Range {
            func_idx,
            il_offset,
        };
        let sl = match self.ranges.binary_search(&range) {
            Ok(idx) => self.source_locations.get(idx)?,
            Err(idx) => {
                let idx = idx.checked_sub(1)?;
                let range = self.ranges.get(idx)?;
                if range.func_idx < func_idx {
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
