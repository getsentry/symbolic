//! The raw SymCache binary file format internals.
//!
use symbolic_common::{Arch, DebugId};

/// The magic file preamble as individual bytes.
const SYMCACHE_MAGIC_BYTES: [u8; 4] = *b"SYMC";

/// The magic file preamble to identify SymCache files.
///
/// Serialized as ASCII "SYMC" on little-endian (x64) systems.
pub(crate) const SYMCACHE_MAGIC: u32 = u32::from_le_bytes(SYMCACHE_MAGIC_BYTES);
/// The byte-flipped magic, which indicates an endianness mismatch.
pub(crate) const SYMCACHE_MAGIC_FLIPPED: u32 = SYMCACHE_MAGIC.swap_bytes();

/// This [`SourceLocation`] is a sentinel value that says that no source location is present here.
/// This is used to push an "end" range that does not resolve to a valid source location.
/// Otherwise, the ranges would implicitly extend to infinity.
pub(crate) const NO_SOURCE_LOCATION: SourceLocation = SourceLocation {
    file_idx: u32::MAX,
    line: u32::MAX,
    function_idx: u32::MAX,
    inlined_into_idx: u32::MAX,
};

/// The header of a symcache file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct Header {
    /// The file magic representing the file format and endianness.
    pub(crate) magic: u32,
    /// The SymCache Format Version.
    pub(crate) version: u32,

    /// Debug identifier of the object file.
    pub(crate) debug_id: DebugId,
    /// CPU architecture of the object file.
    pub(crate) arch: Arch,

    /// Number of included [`File`]s.
    pub(crate) num_files: u32,
    /// Number of included [`Function`]s.
    pub(crate) num_functions: u32,
    /// Number of included [`SourceLocation`]s.
    pub(crate) num_source_locations: u32,
    /// Number of included [`Range`]s.
    pub(crate) num_ranges: u32,
    /// Total number of bytes used for string data.
    pub(crate) string_bytes: u32,

    /// Some reserved space in the header for future extensions that would not require a
    /// completely new parsing method.
    pub(crate) _reserved: [u8; 16],
}

/// Serialized Function metadata in the SymCache.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct Function {
    /// The functions name (reference to a [`String`]).
    pub(crate) name_offset: u32,
    /// The compilation directory (reference to a [`String`]).
    ///
    /// This is retained for binary compatibility; all path information
    /// is contained in [`File`].
    pub(crate) _comp_dir_offset: u32,
    /// The first address covered by this function.
    pub(crate) entry_pc: u32,
    /// The language of the function.
    pub(crate) lang: u32,
}

/// Serialized File in the SymCache.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct File {
    /// The optional compilation directory prefix (reference to a [`String`]).
    pub(crate) comp_dir_offset: u32,
    /// The optional directory prefix (reference to a [`String`]).
    pub(crate) directory_offset: u32,
    /// The file path (reference to a [`String`]).
    pub(crate) name_offset: u32,
}

/// A location in a source file, comprising a file, a line, a function, and
/// the index of the source location this was inlined into, if any.
///
/// Note that each time a function is inlined, as well as the non-inlined
/// version of the function, is represented by a distinct `SourceLocation`.
/// These `SourceLocation`s will all point to the same file, line, and function,
/// but have different inline information.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct SourceLocation {
    /// The optional source file (reference to a [`File`]).
    pub(crate) file_idx: u32,
    /// The line number.
    pub(crate) line: u32,
    /// The function (reference to a [`Function`]).
    pub(crate) function_idx: u32,
    /// The caller source location in case this location was inlined
    /// (reference to another [`SourceLocation`]).
    pub(crate) inlined_into_idx: u32,
}

/// A representation of a code range in the SymCache.
///
/// We only save the start address, the end is implicitly given
/// by the next range's start.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct Range(pub(crate) u32);

/// Returns the amount left to add to the remainder to get 8 if
/// `to_align` isn't a multiple of 8.
pub(crate) fn align_to_eight(to_align: usize) -> usize {
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
        assert_eq!(mem::size_of::<Header>(), 80);
        assert_eq!(mem::align_of::<Header>(), 4);

        assert_eq!(mem::size_of::<Function>(), 16);
        assert_eq!(mem::align_of::<Function>(), 4);

        assert_eq!(mem::size_of::<File>(), 12);
        assert_eq!(mem::align_of::<File>(), 4);

        assert_eq!(mem::size_of::<SourceLocation>(), 16);
        assert_eq!(mem::align_of::<SourceLocation>(), 4);

        assert_eq!(mem::size_of::<Range>(), 4);
        assert_eq!(mem::align_of::<Range>(), 4);
    }
}
