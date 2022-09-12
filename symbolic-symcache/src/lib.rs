//! Provides SymCache support.
//!
//! This includes a reader and writer for the binary format, as well as helper traits and functions
//! to apply transformations to debugging symbols before they are written to the SymCache.
//!
//! # Structure of a SymCache
//!
//! A SymCache (version 7) contains the following primary kinds of data, written in the following
//! order:
//!
//! 1. Files
//! 2. Functions
//! 3. Source Locations
//! 4. Address Ranges
//! 5. String Data
//!
//! The format uses `u32`s to represent line numbers, addresses, references, and string offsets.
//! Line numbers use `0` to represent an unknown or invalid value. Addresses, references, and string
//! offsets instead use `u32::MAX`.
//!
//! Strings are saved in one contiguous section with each individual string prefixed by 4 bytes
//! denoting its length. Functions and files refer to strings by an offset into this string section,
//! hence "string offset".
//!
//! ## Files
//!
//! A file contains string offsets for its file name, parent directory, and compilation directory.
//!
//! ## Functions
//!
//! A function contains string offsets for its name and compilation directory, a u32 for its entry
//! address, and a u32 representing the source language. The name is non-optional, i.e., the name
//! index should always point to a valid string.
//!
//! ## Address Ranges
//!
//! Ranges are saved as a contiguous list of `u32`s, representing their starting addresses.
//!
//! ## Source Locations
//!
//! A source location in a symcache represents a possibly-inlined copy of a line in a source file.
//! It contains a line number, a reference to a file (see above), a reference to a function (ditto),
//! and a reference to the source location into which this source location was inlined. All of these
//! data except for the function are optional.
//!
//! ## Mapping From Ranges To Source Locations
//!
//! Every range in the SymCache is associated with at least one source location. As mentioned above,
//! each source location may in turn have a reference to a source location into which it is inlined.
//! Conceptually, each address range points to a sequence of source locations, representing a
//! hierarchy of inlined function calls.
//!
//! ### Example
//!
//! The mapping
//!
//! - `0x0001 - 0x002f`
//!   - `trigger_crash` in file `b.c`, line 12
//!   - inlined into `main` in file `a.c`, line 10
//! - `0x002f - 0x004a`
//!   - `trigger_crash` in file `b.c`, line 13
//!   - inlined into `main` in file `a.c`, line 10
//!
//! is represented like this in the SymCache (function/file name strings inlined for simplicity):
//! ```text
//! ranges: [
//!     0x0001 -> 1
//!     0x002f -> 2
//! ]
//!
//! source_locations: [{
//!     file: "a.c"
//!     line: 10
//!     function: "main"
//!     inlined_into: u32::MAX (not inlined)
//! }, {
//!     file: "b.c"
//!     line: 12
//!     function: "trigger_crash"
//!     inlined_into: 0 <- index reference to "main"
//! }, {
//!     file: "b.c"
//!     line: 13
//!     function: "trigger_crash"
//!     inlined_into: 0 <- index reference to "main"
//! }]
//! ```
//!
//! # Lookups
//!
//! To look up an address `addr` in a SymCache:
//!
//! 1. Find the range covering `addr` via binary search.
//! 2. Find the source location belonging to this range.
//! 3. Return an iterator over a series of source locations that starts at the source location found
//!    in step 2. The iterator climbs up through the inlining hierarchy, ending at the root source
//!    location.
//!
//! The returned source locations contain accessor methods for their function, file, and line
//! number.

#![warn(missing_docs)]

mod error;
mod lookup;
mod raw;
pub mod transform;
mod writer;

use std::convert::TryInto;

use symbolic_common::Arch;
use symbolic_common::AsSelf;
use symbolic_common::DebugId;
use watto::StringTable;
use watto::{align_to, Pod};

pub use error::{Error, ErrorKind};
pub use lookup::*;
pub use writer::SymCacheConverter;

type Result<T, E = Error> = std::result::Result<T, E>;

/// The latest version of the file format.
///
/// Version history:
///
/// 1: Initial implementation
/// 2: PR #58:  Migrate from UUID to Debug ID
/// 3: PR #148: Consider all PT_LOAD segments in ELF
/// 4: PR #155: Functions with more than 65k line records
/// 5: PR #221: Invalid inlinee nesting leading to wrong stack traces
/// 6: PR #319: Correct line offsets and spacer line records
/// 7: PR #459: A new binary format fundamentally based on addr ranges
/// 8: PR #670: Use LEB128-prefixed string table
pub const SYMCACHE_VERSION: u32 = 8;

/// The serialized SymCache binary format.
///
/// This can be parsed from a binary buffer via [`SymCache::parse`] and lookups on it can be performed
/// via the [`SymCache::lookup`] method.
#[derive(Clone, PartialEq, Eq)]
pub struct SymCache<'data> {
    header: &'data raw::Header,
    files: &'data [raw::File],
    functions: &'data [raw::Function],
    source_locations: &'data [raw::SourceLocation],
    ranges: &'data [raw::Range],
    string_bytes: &'data [u8],
}

impl<'data> std::fmt::Debug for SymCache<'data> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SymCache")
            .field("version", &self.header.version)
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

impl<'data> SymCache<'data> {
    /// Parse the SymCache binary format into a convenient type that allows safe access and
    /// fast lookups.
    pub fn parse(buf: &'data [u8]) -> Result<Self> {
        let (header, rest) = raw::Header::ref_from_prefix(buf).ok_or(ErrorKind::InvalidHeader)?;
        if header.magic == raw::SYMCACHE_MAGIC_FLIPPED {
            return Err(ErrorKind::WrongEndianness.into());
        }
        if header.magic != raw::SYMCACHE_MAGIC {
            return Err(ErrorKind::WrongFormat.into());
        }
        if header.version != SYMCACHE_VERSION && header.version != 7 {
            return Err(ErrorKind::WrongVersion.into());
        }

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

        Ok(SymCache {
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
        if self.header.version >= 8 {
            // version >= 8: string length prefixes are LEB128
            StringTable::read(self.string_bytes, offset as usize).ok()
        } else {
            // version < 8: string length prefixes are u32
            if offset == u32::MAX {
                return None;
            }
            let len_offset = offset as usize;
            let len_size = std::mem::size_of::<u32>();
            let len = u32::from_ne_bytes(
                self.string_bytes
                    .get(len_offset..len_offset + len_size)?
                    .try_into()
                    .unwrap(),
            ) as usize;

            let start_offset = len_offset + len_size;
            let end_offset = start_offset + len;
            let bytes = self.string_bytes.get(start_offset..end_offset)?;

            std::str::from_utf8(bytes).ok()
        }
    }

    /// The version of the SymCache file format.
    pub fn version(&self) -> u32 {
        self.header.version
    }

    /// Returns true if this symcache's version is the current version of the format.
    pub fn is_latest(&self) -> bool {
        self.header.version == SYMCACHE_VERSION
    }

    /// The architecture of the symbol file.
    pub fn arch(&self) -> Arch {
        self.header.arch
    }

    /// The debug identifier of the cache file.
    pub fn debug_id(&self) -> DebugId {
        self.header.debug_id
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for SymCache<'d> {
    type Ref = SymCache<'slf>;

    fn as_self(&'slf self) -> &Self::Ref {
        self
    }
}
