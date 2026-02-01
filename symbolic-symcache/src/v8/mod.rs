pub(crate) mod lookup;

// V8 uses the same raw format as v7.
use crate::raw::v7 as raw;
use crate::{ErrorKind, Result};

use watto::{align_to, Pod, StringTable};

/// The serialized SymCache V8 binary format.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SymCacheV8<'data> {
    pub(crate) header: &'data raw::Header,
    pub(crate) files: &'data [raw::File],
    pub(crate) functions: &'data [raw::Function],
    pub(crate) source_locations: &'data [raw::SourceLocation],
    pub(crate) ranges: &'data [raw::Range],
    pub(crate) string_bytes: &'data [u8],
}

impl std::fmt::Debug for SymCacheV8<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SymCache")
            .field("version", &8)
            .field("debug_id", &self.header.debug_id)
            .field("arch", &self.header.arch)
            .field("files", &self.header.num_files)
            .field("functions", &self.header.num_functions)
            .field("source_locations", &self.header.num_source_locations)
            .field("ranges", &self.header.num_ranges)
            .field("string_bytes", &self.header.string_bytes)
            .finish()
    }
}

impl<'data> SymCacheV8<'data> {
    pub fn parse(buf: &'data [u8]) -> Result<Self> {
        let (header, rest) = raw::Header::ref_from_prefix(buf).ok_or(ErrorKind::InvalidHeader)?;

        let (_, rest) = align_to(rest, 8).ok_or(ErrorKind::InvalidFiles)?;
        let (files, rest) = raw::File::slice_from_prefix(rest, header.num_files as usize)
            .ok_or(ErrorKind::InvalidFiles)?;

        let (_, rest) = align_to(rest, 8).ok_or(ErrorKind::InvalidFunctions)?;
        let (functions, rest) =
            raw::Function::slice_from_prefix(rest, header.num_functions as usize)
                .ok_or(ErrorKind::InvalidFunctions)?;

        let (_, rest) = align_to(rest, 8).ok_or(ErrorKind::InvalidSourceLocations)?;
        let (source_locations, rest) =
            raw::SourceLocation::slice_from_prefix(rest, header.num_source_locations as usize)
                .ok_or(ErrorKind::InvalidSourceLocations)?;

        let (_, rest) = align_to(rest, 8).ok_or(ErrorKind::InvalidRanges)?;
        let (ranges, rest) = raw::Range::slice_from_prefix(rest, header.num_ranges as usize)
            .ok_or(ErrorKind::InvalidRanges)?;

        let (_, rest) = align_to(rest, 8).ok_or(ErrorKind::UnexpectedStringBytes {
            expected: header.string_bytes as usize,
            found: 0,
        })?;
        if rest.len() < header.string_bytes as usize {
            return Err(ErrorKind::UnexpectedStringBytes {
                expected: header.string_bytes as usize,
                found: rest.len(),
            }
            .into());
        }

        Ok(Self {
            header,
            files,
            functions,
            source_locations,
            ranges,
            string_bytes: rest,
        })
    }

    /// Resolves a string reference to the pointed-to `&str` data.
    fn get_string(&self, offset: u32) -> Option<&'data str> {
        // version >= 8: string length prefixes are LEB128
        StringTable::read(self.string_bytes, offset as usize).ok()
    }
}
