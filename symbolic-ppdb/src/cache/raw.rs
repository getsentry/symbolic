use zerocopy::FromBytes;

/// The magic file preamble as individual bytes.
const PPDBCACHE_MAGIC_BYTES: [u8; 4] = *b"PDBC";

/// The magic file preamble to identify SymCache files.
///
/// Serialized as ASCII "SYMC" on little-endian (x64) systems.
pub(crate) const PPDBCACHE_MAGIC: u32 = u32::from_le_bytes(PPDBCACHE_MAGIC_BYTES);
/// The byte-flipped magic, which indicates an endianness mismatch.
pub(crate) const PPDBCACHE_MAGIC_FLIPPED: u32 = PPDBCACHE_MAGIC.swap_bytes();

#[derive(Debug, Clone, FromBytes)]
#[repr(C)]
pub(crate) struct Header {
    pub(crate) magic: u32,
    pub(crate) version: u32,
    pub(crate) pdb_id: [u8; 20],
    pub(crate) num_ranges: u32,
    pub(crate) string_bytes: u32,
    pub(crate) _reserved: [u8; 16],
}

#[derive(Debug, Clone, Copy, FromBytes)]
#[repr(C)]
pub(crate) struct SourceLocation {
    pub(crate) line: u32,
    pub(crate) file_name_idx: u32,
    pub(crate) lang: u32,
}

#[derive(Debug, Clone, Copy, FromBytes, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub(crate) struct Range {
    pub(crate) idx: u32,
    pub(crate) il_offset: u32,
}
