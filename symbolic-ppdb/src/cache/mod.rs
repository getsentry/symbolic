//! Provides PortablePdbCache support.
//!
//! This includes a reader and writer for the binary format.
//!
//! # Structure of a PortablePdbCache
//!
//! A PortablePdbCache(version 1) contains the following primary kinds of data, written in the following
//! order:
//!
//! 1. Source Locations
//! 2. Address Ranges
//! 3. String Data
//!
//! The format uses `u32`s to represent line numbers, IL offsets,  and string offsets.
//! Line numbers use `0` to represent an unknown or invalid value. String
//! offsets instead use `u32::MAX`.
//!
//! Strings are saved in one contiguous section with each individual string prefixed by
//! its length in LEB-128 encoding. Source locations refer to strings by an offset into this string section,
//! hence "string offset".
//!
//! ## Address Ranges
//!
//! Ranges are saved as a contiguous list of pairs of `u32`s, the first representing the index of the function
//! the range belongs to and the second representing the range's starting IL offset. Ranges are ordered
//! by function index and then by starting offset.
//!
//! ## Source Locations
//!
//! A source location in a PortablePDBCache represents a line in a source file.
//! It contains a line number, a reference to a file name (see above), and a `u32` representing the source file's language.
//!
//! ## Mapping From Ranges To Source Locations
//!
//! The mapping from ranges to source locations is one-to-one: the `i`th range in the cache corresponds to the `i`th source location.
//!
//! # Lookups
//!
//! To look up an IL offset `offset` for the `i`th function in a PortablePdbCache:
//!
//! 1. Find the range belonging to the `i`th function that covers `offset` via binary search.
//! 2. Find the source location belonging to this range.

pub(crate) mod lookup;
pub(crate) mod raw;
pub(crate) mod writer;

use thiserror::Error;
use zerocopy::LayoutVerified;

const PPDBCACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Error)]
pub enum CacheErrorKind {
    #[error("could not read header")]
    InvalidHeader,
    #[error("wrong endianness")]
    WrongEndianness,
    #[error("invalid magic: {0}")]
    InvalidMagic(u32),
    #[error("wrong version: {0}")]
    WrongVersion(u32),
    #[error("could not read ranges")]
    InvalidRanges,
    #[error("could not read source locations")]
    InvalidSourceLocations,
    #[error("expected {expected} string bytes, found {found}")]
    UnexpectedStringBytes { expected: usize, found: usize },
    #[error("error processing portable pdb file")]
    PortablePdb,
}

#[derive(Debug, Error)]
#[error("{kind}")]
pub struct CacheError {
    pub(crate) kind: CacheErrorKind,
    #[source]
    pub(crate) source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl CacheError {
    /// Creates a new SymCache error from a known kind of error as well as an
    /// arbitrary error payload.
    pub(crate) fn new<E>(kind: CacheErrorKind, source: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let source = Some(source.into());
        Self { kind, source }
    }

    /// Returns the corresponding [`ErrorKind`] for this error.
    pub fn kind(&self) -> CacheErrorKind {
        self.kind
    }
}

impl From<CacheErrorKind> for CacheError {
    fn from(kind: CacheErrorKind) -> Self {
        Self { kind, source: None }
    }
}

impl From<crate::Error> for CacheError {
    fn from(e: crate::Error) -> Self {
        Self::new(CacheErrorKind::PortablePdb, e)
    }
}

/// The serialized PortablePdbCache binary format.
///
/// This can be parsed from a binary buffer via [`PortablePdbCache::parse`] and lookups on it can be performed
/// via the [`PortablePdbCache::lookup`] method.
pub struct PortablePdbCache<'data> {
    header: &'data raw::Header,
    source_locations: &'data [raw::SourceLocation],
    ranges: &'data [raw::Range],
    string_bytes: &'data [u8],
}

impl<'data> PortablePdbCache<'data> {
    /// Parses the given buffer into a `PortablePdbCache`.
    pub fn parse(buf: &'data [u8]) -> Result<Self, CacheError> {
        let (lv, rest) = LayoutVerified::<_, raw::Header>::new_from_prefix(buf)
            .ok_or(CacheErrorKind::InvalidHeader)?;

        let header = lv.into_ref();

        if header.magic == raw::PPDBCACHE_MAGIC_FLIPPED {
            return Err(CacheErrorKind::WrongEndianness.into());
        }
        if header.magic != raw::PPDBCACHE_MAGIC {
            return Err(CacheErrorKind::InvalidMagic(header.magic).into());
        }

        if header.version != PPDBCACHE_VERSION {
            return Err(CacheErrorKind::WrongVersion(header.version).into());
        }

        let rest = align_buf(rest);

        let (lv, rest) = LayoutVerified::<_, [raw::SourceLocation]>::new_slice_from_prefix(
            rest,
            header.num_ranges as usize,
        )
        .ok_or(CacheErrorKind::InvalidSourceLocations)?;

        let source_locations = lv.into_slice();
        let rest = align_buf(rest);

        let (lv, rest) = LayoutVerified::<_, [raw::Range]>::new_slice_from_prefix(
            rest,
            header.num_ranges as usize,
        )
        .ok_or(CacheErrorKind::InvalidRanges)?;

        let ranges = lv.into_slice();
        let rest = align_buf(rest);

        if rest.len() < header.string_bytes as usize {
            return Err(CacheErrorKind::UnexpectedStringBytes {
                expected: header.string_bytes as usize,
                found: rest.len(),
            }
            .into());
        }

        Ok(Self {
            header,
            source_locations,
            ranges,
            string_bytes: rest,
        })
    }
}

impl<'data> std::fmt::Debug for PortablePdbCache<'data> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortablePdbCache")
            .field("version", &self.header.version)
            .field("pdb_id", &self.header.pdb_id)
            .field("ranges", &self.header.num_ranges)
            .field("string_bytes", &self.header.string_bytes)
            .finish()
    }
}

fn align_buf(buf: &[u8]) -> &[u8] {
    let offset = buf.as_ptr().align_offset(8);
    buf.get(offset..).unwrap_or(&[])
}
