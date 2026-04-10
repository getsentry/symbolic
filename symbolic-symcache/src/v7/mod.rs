pub(crate) mod lookup;

use std::marker::PhantomData;

use crate::raw::v7 as raw;
use crate::{ErrorKind, Result};

use symbolic_common::Arch;
use watto::{align_to, Pod, StringTable};

/// The serialized SymCache V7 binary format.
///
/// The `Flavor` type parameter controls how strings
/// are parsed from the contained string bytes (via the
/// [`GetString`] trait):
/// - `V7`: String length prefixes are always 4B.
/// - `V8`: String length prefixes are variable-length.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SymCacheV7Inner<'data, Flavor = V8> {
    pub(crate) header: &'data raw::Header,
    pub(crate) files: &'data [raw::File],
    pub(crate) functions: &'data [raw::Function],
    pub(crate) source_locations: &'data [raw::SourceLocation],
    pub(crate) ranges: &'data [raw::Range],
    pub(crate) string_bytes: &'data [u8],
    _d: PhantomData<Flavor>,
}

impl<Flavor: SymCacheV7Flavor> std::fmt::Debug for SymCacheV7Inner<'_, Flavor> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SymCache")
            .field("version", &Flavor::VERSION)
            .field("debug_id", &self.header.debug_id)
            .field("arch", &Arch::from_u32(self.header.arch))
            .field("files", &self.header.num_files)
            .field("functions", &self.header.num_functions)
            .field("source_locations", &self.header.num_source_locations)
            .field("ranges", &self.header.num_ranges)
            .field("string_bytes", &self.header.string_bytes)
            .finish()
    }
}

impl<'data, Flavor> SymCacheV7Inner<'data, Flavor> {
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
            _d: PhantomData,
        })
    }
}

impl<'data, Flavor: SymCacheV7Flavor> SymCacheV7Inner<'data, Flavor> {
    /// Resolves a string reference to the pointed-to `&str` data.
    fn get_string(&self, offset: u32) -> Option<&'data str> {
        Flavor::get_string(self.string_bytes, offset)
    }
}

pub(crate) trait SymCacheV7Flavor {
    /// The version number that should be reported for this flavor.
    const VERSION: u32;
    /// Resolves a string reference to the pointed-to `&str` data.
    fn get_string(string_bytes: &[u8], offset: u32) -> Option<&str>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct V7;

impl SymCacheV7Flavor for V7 {
    fn get_string(string_bytes: &[u8], offset: u32) -> Option<&str> {
        // version < 8: string length prefixes are u32
        if offset == u32::MAX {
            return None;
        }
        let len_offset = offset as usize;
        let len_size = std::mem::size_of::<u32>();
        let len = u32::from_ne_bytes(
            string_bytes
                .get(len_offset..len_offset + len_size)?
                .try_into()
                .unwrap(),
        ) as usize;

        let start_offset = len_offset + len_size;
        let end_offset = start_offset + len;
        let bytes = string_bytes.get(start_offset..end_offset)?;

        std::str::from_utf8(bytes).ok()
    }

    const VERSION: u32 = 7;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct V8;

impl SymCacheV7Flavor for V8 {
    fn get_string(string_bytes: &[u8], offset: u32) -> Option<&str> {
        StringTable::read(string_bytes, offset as usize).ok()
    }

    const VERSION: u32 = 8;
}

pub(crate) type SymCacheV7<'data> = SymCacheV7Inner<'data, V7>;
pub(crate) type SymCacheV8<'data> = SymCacheV7Inner<'data, V8>;
