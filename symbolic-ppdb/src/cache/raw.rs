use zerocopy::{AsBytes, FromBytes};

/// The magic file preamble as individual bytes.
const PPDBCACHE_MAGIC_BYTES: [u8; 4] = *b"PDBC";

/// The magic file preamble to identify PortablePdbCache files.
///
/// Serialized as ASCII "PDBC" on little-endian (x64) systems.
pub(crate) const PPDBCACHE_MAGIC: u32 = u32::from_le_bytes(PPDBCACHE_MAGIC_BYTES);
/// The byte-flipped magic, which indicates an endianness mismatch.
pub(crate) const PPDBCACHE_MAGIC_FLIPPED: u32 = PPDBCACHE_MAGIC.swap_bytes();

/// The header of a PortablePdbCache file.
#[derive(Debug, Clone, FromBytes, AsBytes)]
#[repr(C)]
pub(crate) struct Header {
    /// The file magic representing the file format and endianness.
    pub(crate) magic: u32,
    /// The PortablePdbCache format version.
    pub(crate) version: u32,
    /// A byte sequence uniquely representing the debugging metadata blob content.
    pub(crate) pdb_id: [u8; 20],
    /// The number of files contained in the cache file.
    pub(crate) num_files: u32,
    /// The number of ranges/source locations contained in the cache file.
    pub(crate) num_ranges: u32,
    /// Total number of bytes used for string data.
    pub(crate) string_bytes: u32,
    /// Some reserved space in the header for future extensions that would not require a
    /// completely new parsing method.
    pub(crate) _reserved: [u8; 16],
}

/// A location in a source file, comprising a line and the index of a file.
#[derive(Debug, Clone, Copy, FromBytes, AsBytes)]
#[repr(C)]
pub(crate) struct SourceLocation {
    pub(crate) line: u32,
    pub(crate) file_idx: u32,
}

/// A range of IL offsets in a function.
///
/// Only the starting IL offset is saved; the ending offset is given implicitly by
/// the starting offset of the next range (if any).
#[derive(Debug, Clone, Copy, FromBytes, AsBytes, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub(crate) struct Range {
    pub(crate) func_idx: u32,
    pub(crate) il_offset: u32,
}

/// Serialized file in the cache.
#[derive(Debug, Clone, Copy, FromBytes, AsBytes, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub(crate) struct File {
    /// The file path (reference to a [`String`]).
    pub(crate) name_offset: u32,
    /// The file's source language.
    pub(crate) lang: u32,
}
