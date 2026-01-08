//! Provides SymCache support.
//!
//! This includes a reader and writer for the binary format, as well as helper traits and functions
//! to apply transformations to debugging symbols before they are written to the SymCache.
//!
//! # Structure of a SymCache
//!
//! A SymCache (version 9) contains the following primary kinds of data, written in the following
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
//! In version 9+, files also contain an optional VCS revision string offset.
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
mod v7;
mod v8;
mod v9;
mod writer;

use symbolic_common::Arch;
use symbolic_common::AsSelf;
use symbolic_common::DebugId;
use watto::Pod;

pub use error::{Error, ErrorKind};
pub use lookup::*;
pub use writer::SymCacheConverter;

use crate::v7::SymCacheV7;
use crate::v8::SymCacheV8;
use crate::v9::SymCacheV9;

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
/// 9: PR #943: Add revision_offset field to File structure for VCS revision tracking
pub const SYMCACHE_VERSION: u32 = 9;

/// The serialized SymCache binary format.
///
/// This can be parsed from a binary buffer via [`SymCache::parse`] and lookups on it can be performed
/// via the [`SymCache::lookup`] method.
#[derive(Clone, PartialEq, Eq)]
pub struct SymCache<'data> {
    version: &'data raw::VersionInfo,
    inner: SymCacheInner<'data>,
}

impl std::fmt::Debug for SymCache<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.inner {
            SymCacheInner::V7(ref sym_cache_v7) => sym_cache_v7.fmt(f),
            SymCacheInner::V8(ref sym_cache_v8) => sym_cache_v8.fmt(f),
            SymCacheInner::V9(ref sym_cache_v9) => sym_cache_v9.fmt(f),
        }
    }
}

impl<'data> SymCache<'data> {
    /// Parse the SymCache binary format into a convenient type that allows safe access and
    /// fast lookups.
    pub fn parse(buf: &'data [u8]) -> Result<Self> {
        let (version, rest) =
            raw::VersionInfo::ref_from_prefix(buf).ok_or(ErrorKind::InvalidHeader)?;
        if version.magic == raw::SYMCACHE_MAGIC_FLIPPED {
            return Err(ErrorKind::WrongEndianness.into());
        }
        if version.magic != raw::SYMCACHE_MAGIC {
            return Err(ErrorKind::WrongFormat.into());
        }

        let inner = match version.version {
            7 => SymCacheInner::V7(SymCacheV7::parse(rest)?),
            8 => SymCacheInner::V8(SymCacheV8::parse(rest)?),
            9 => SymCacheInner::V9(SymCacheV9::parse(rest)?),
            _ => return Err(ErrorKind::WrongVersion.into()),
        };

        Ok(Self { version, inner })
    }

    /// The version of the SymCache file format.
    pub fn version(&self) -> u32 {
        self.version.version
    }

    /// Returns true if this symcache's version is the current version of the format.
    pub fn is_latest(&self) -> bool {
        self.version.version == SYMCACHE_VERSION
    }

    /// The architecture of the symbol file.
    pub fn arch(&self) -> Arch {
        match &self.inner {
            SymCacheInner::V7(cache) => cache.header.arch,
            SymCacheInner::V8(cache) => cache.header.arch,
            SymCacheInner::V9(cache) => cache.header.arch,
        }
    }

    /// The debug identifier of the cache file.
    pub fn debug_id(&self) -> DebugId {
        match &self.inner {
            SymCacheInner::V7(cache) => cache.header.debug_id,
            SymCacheInner::V8(cache) => cache.header.debug_id,
            SymCacheInner::V9(cache) => cache.header.debug_id,
        }
    }
}

impl<'slf, 'd: 'slf> AsSelf<'slf> for SymCache<'d> {
    type Ref = SymCache<'slf>;

    fn as_self(&'slf self) -> &'slf Self::Ref {
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SymCacheInner<'data> {
    V7(SymCacheV7<'data>),
    V8(SymCacheV8<'data>),
    V9(SymCacheV9<'data>),
}
