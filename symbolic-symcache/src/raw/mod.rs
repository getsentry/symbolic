//! The raw SymCache binary file format internals.
//!

pub(crate) mod v7;

use watto::Pod;

/// The magic file preamble as individual bytes.
const SYMCACHE_MAGIC_BYTES: [u8; 4] = *b"SYMC";

/// The magic file preamble to identify SymCache files.
///
/// Serialized as ASCII "SYMC" on little-endian (x64) systems.
pub(crate) const SYMCACHE_MAGIC: u32 = u32::from_le_bytes(SYMCACHE_MAGIC_BYTES);
/// The byte-flipped magic, which indicates an endianness mismatch.
pub(crate) const SYMCACHE_MAGIC_FLIPPED: u32 = SYMCACHE_MAGIC.swap_bytes();

/// Minimal preamble containing just magic bytes and version number.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct VersionInfo {
    /// The file magic representing the file format and endianness.
    pub(crate) magic: u32,
    /// The SymCache Format Version.
    pub(crate) version: u32,
}

unsafe impl Pod for VersionInfo {}

#[cfg(test)]
mod tests {
    use std::mem;

    use super::*;

    #[test]
    fn test_sizeof() {
        assert_eq!(mem::size_of::<VersionInfo>(), 8);
        assert_eq!(mem::align_of::<VersionInfo>(), 4);
    }
}
