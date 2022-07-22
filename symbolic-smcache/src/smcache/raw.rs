use zerocopy::{AsBytes, FromBytes};

/// The magic file preamble as individual bytes.
const SMCACHE_MAGIC_BYTES: [u8; 4] = *b"SMCA";

/// The magic file preamble to identify SmCache files.
///
/// Serialized as ASCII "SMCA" on little-endian (x64) systems.
pub const SMCACHE_MAGIC: u32 = u32::from_le_bytes(SMCACHE_MAGIC_BYTES);
/// The byte-flipped magic, which indicates an endianness mismatch.
pub const SMCACHE_MAGIC_FLIPPED: u32 = SMCACHE_MAGIC.swap_bytes();

/// The current Format version
pub const SMCACHE_VERSION: u32 = 1;

/// The SmCache binary Header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromBytes, AsBytes)]
#[repr(C)]
pub struct Header {
    /// The file magic representing the file format and endianness.
    pub magic: u32,
    /// The Format Version.
    pub version: u32,

    /// The number of mappings covered by this file.
    pub num_mappings: u32,

    /// The number of original source files.
    pub num_files: u32,

    /// The number of original names.
    pub num_names: u32,

    /// The total number of line offsets.
    pub num_line_offsets: u32,

    /// The total number of bytes in the string table.
    pub string_bytes: u32,

    /// Some reserved space in the header for future extensions that would not require a
    /// completely new parsing method.
    pub _reserved: [u8; 8],
}

/// A minified source position of line/column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, FromBytes, AsBytes)]
#[repr(C)]
pub struct MinifiedSourcePosition {
    pub line: u32,
    pub column: u32,
}

impl From<crate::source::SourcePosition> for MinifiedSourcePosition {
    fn from(sp: crate::source::SourcePosition) -> Self {
        Self {
            line: sp.line,
            column: sp.column,
        }
    }
}

/// Sentinel value used to denote unknown file.
pub const NO_FILE_SENTINEL: u32 = u32::MAX;
/// Sentinel value used to denote unknown/global scope.
pub const GLOBAL_SCOPE_SENTINEL: u32 = u32::MAX;
/// Sentinel value used to denote anonymous function scope.
pub const ANONYMOUS_SCOPE_SENTINEL: u32 = u32::MAX - 1;

/// The original source location, file line and scope.
#[derive(Clone, Copy, Debug, PartialEq, FromBytes, AsBytes)]
#[repr(C)]
pub struct OriginalSourceLocation {
    /// The optional original source file (index in the file table).
    pub file_idx: u32,
    /// The original line number.
    pub line: u32,
    /// The optional scope name (offset into string table).
    pub scope_idx: u32,
}

/// A minified source position of line/column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, FromBytes, AsBytes)]
#[repr(C)]
pub struct File {
    /// The source filename (offset into string table).
    pub name_offset: u32,
    /// The file contents (offset into string table).
    pub source_offset: u32,
    /// Start of the line offsets (index into line offsets table).
    pub line_offsets_start: u32,
    /// End of the line offsets (index into line offsets table).
    pub line_offsets_end: u32,
}

/// An offset into each files content representing line boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, FromBytes, AsBytes)]
#[repr(C)]
pub struct LineOffset(pub u32);

/// Returns the amount left to add to the remainder to get 8 if
/// `to_align` isn't a multiple of 8.
pub fn align_to_eight(to_align: usize) -> usize {
    let remainder = to_align % 8;
    if remainder == 0 {
        remainder
    } else {
        8 - remainder
    }
}

#[cfg(test)]
mod tests {
    use std::mem;

    use super::*;

    #[test]
    fn test_sizeof() {
        assert_eq!(mem::size_of::<Header>(), 32);
        assert_eq!(mem::align_of::<Header>(), 4);

        assert_eq!(mem::size_of::<MinifiedSourcePosition>(), 8);
        assert_eq!(mem::align_of::<MinifiedSourcePosition>(), 4);

        assert_eq!(mem::size_of::<OriginalSourceLocation>(), 12);
        assert_eq!(mem::align_of::<OriginalSourceLocation>(), 4);
    }
}
