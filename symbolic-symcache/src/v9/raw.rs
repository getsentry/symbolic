//! The raw SymCache V9 binary file format internals.
//!
//! V9 adds source server information support to the File structure.

use std::fmt;

use watto::Pod;

use symbolic_common::DebugId;

/// The current [`Header::variable_header`] version.
///
/// On a version mismatch, the sections referenced in the [`VariableHeader`] must not be parsed.
pub const VARIABLE_HEADER_VERSION: u16 = 1;

/// This [`FunctionVariables`] is a sentinel for functions without variables.
pub const NO_VARIABLES: FunctionVariables = FunctionVariables {
    variable_idx: u32::MAX,
    num_variables: 0,
};

/// This [`SourceLocation`] is a sentinel value that says that no source location is present here.
/// This is used to push an "end" range that does not resolve to a valid source location.
/// Otherwise, the ranges would implicitly extend to infinity.
pub const NO_SOURCE_LOCATION: SourceLocation = SourceLocation {
    file_idx: u32::MAX,
    line: u32::MAX,
    function_idx: u32::MAX,
    inlined_into_idx: u32::MAX,
};

/// The header of a symcache file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Header {
    /// Debug identifier of the object file.
    pub debug_id: DebugId,
    /// CPU architecture of the object file.
    ///
    /// This cannot be [`symbolic_common::Arch`] because
    /// not every bit pattern is valid for that type.
    pub arch: u32,
    /// Number of included [`File`]s.
    pub num_files: u32,
    /// Number of included [`Function`]s.
    pub num_functions: u32,
    /// Number of included [`SourceLocation`]s.
    pub num_source_locations: u32,
    /// Number of included [`Range`]s.
    pub num_ranges: u32,
    /// Total number of bytes used for string data.
    pub string_bytes: u32,

    /// Version of the optional [`VariableHeader`].
    ///
    /// If the version is `0` there is no variable header. This allows incompatible changes
    /// to variable and type information in a symcache without requiring a new symcache version.
    ///
    /// Once the format is stable, the variable header will no longer be optional in symcache
    /// version 10.
    pub variable_header: u16,

    /// Some reserved space in the header for future extensions that would not require a
    /// completely new parsing method.
    pub _reserved: [u8; 14],
}

/// Optional header appended to the [`Header`] if the symcache contains variable information.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct VariableHeader {
    /// Number of included [`Variable`]'s.
    pub num_variables: u32,
    /// Number of included [`VariableLocationInfo`]'s.
    pub num_variable_locations: u32,
    /// Number of included [`Type`]'s.
    pub num_types: u32,
}

/// Serialized Function metadata in the SymCache.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct Function {
    /// The functions name (reference to a [`String`]).
    pub name_offset: u32,
    /// The compilation directory (reference to a [`String`]).
    ///
    /// This is retained for binary compatibility; all path information
    /// is contained in [`File`].
    pub _comp_dir_offset: u32,
    /// The first address covered by this function.
    pub entry_pc: u32,
    /// The language of the function.
    pub lang: u32,
}

/// Function variable metadata stored in a separate section.
///
/// An index into [`Function`]'s can also be used to lookup function variable information from this
/// section.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct FunctionVariables {
    /// The variable index of the first variable.
    pub variable_idx: u32,
    /// The amount of variables associated with the function.
    pub num_variables: u32,
}

/// Serialized File in the SymCache (V9 format with revision support).
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct File {
    /// The optional compilation directory prefix (reference to a [`String`]).
    pub comp_dir_offset: u32,
    /// The optional directory prefix (reference to a [`String`]).
    pub directory_offset: u32,
    /// The file path (reference to a [`String`]).
    pub name_offset: u32,
    /// The optional base name on the source server (reference to a [`String`]).
    ///
    /// This field was added in version 9.
    pub srcsrv_name_offset: u32,
    /// The optional path to the file on the source server (reference to a [`String`]).
    ///
    /// This field was added in version 9.
    pub srcsrv_dir_offset: u32,
    /// The optional VCS revision (reference to a [`String`]).
    ///
    /// This field was added in version 9.
    pub srcsrv_revision_offset: u32,
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
pub struct SourceLocation {
    /// The optional source file (reference to a [`File`]).
    pub file_idx: u32,
    /// The line number.
    pub line: u32,
    /// The function (reference to a [`Function`]).
    pub function_idx: u32,
    /// The caller source location in case this location was inlined
    /// (reference to another [`SourceLocation`]).
    pub inlined_into_idx: u32,
}

/// A representation of a code range in the SymCache.
///
/// We only save the start address, the end is implicitly given
/// by the next range's start.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct Range(pub u32);

/// A representation of a variable.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct Variable {
    /// The variable name (reference to a [`String`]).
    pub name_offset: u32,
    /// The type (reference to a [`Type`]).
    pub type_idx: u32,
    /// The first location associated with the variable (reference to a [`VariableLocationInfo`]).
    pub location_idx: u32,
    /// The amount of locations.
    pub num_locations: u32,
    /// The inlined depth of the owning function.
    ///
    /// If `0` the variable belongs to the outermost function. This is essentially a way to track
    /// scopes, limited to function scopes. Each level of depth is a newly nested scope.
    ///
    /// To recover which variables belong to a [`SourceLocation`], the depth of the source location
    /// must first be computed by following [`SourceLocation::inlined_into_idx`], each step increases
    /// the depth by one.
    /// Knowing the depth and the outermost function, the list of variables can then be filtered down
    /// to only variables at this depth.
    pub depth: u8,
    /// The variable kind.
    ///
    /// The variable kind is required and must not be [`u8::MAX`].
    pub kind: u8,
    pub _reserved: [u8; 2],
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct RegisterLocation {
    pub id: u16,
    pub _reserved: [u8; 2],
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct FrameOffsetLocation {
    pub offset: i32,
}

pub mod location {
    use super::*;

    crate::utils::tagged_union! {
        register: RegisterLocation,
        frame_offset: FrameOffsetLocation,
    }
}

/// A representation of a variable location.
#[derive(Clone)]
#[repr(C)]
pub struct VariableLocationInfo {
    /// First instruction address (pc) where this location is valid.
    pub start: u32,
    /// Size of the location, indicating the first instruction address where this location is no longer valid.
    pub size: u32,
    /// The location data.
    pub location: location::Impl,
    /// The location kind.
    pub kind: u8,
    pub _reserved: [u8; 3],
}

impl VariableLocationInfo {
    pub fn location(&self) -> Option<location::Enum> {
        self.location.into_enum(self.kind)
    }
}

impl fmt::Debug for VariableLocationInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VariableLocationInfo")
            .field("start", &self.start)
            .field("size", &self.size)
            .field("location", &self.location.tagged(self.kind))
            .field("_reserved", &self._reserved)
            .finish()
    }
}

/// A representation of a primitive type.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct PrimitiveType {
    /// The type name (reference to a [`String`]).
    pub name_offset: u32,
    /// The size of the type in memory.
    pub size: u32,
    /// The encoding of the type.
    pub encoding: u8,
    /// Padding.
    pub _reserved: [u8; 3],
}

/// A representation of a pointer type.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct PointerType {
    /// The type this pointer points to (reference to a [`Type`]).
    pub pointee_idx: u32,
    /// The size/width of the pointer.
    pub size: u32,
    /// Padding.
    pub _reserved: [u8; 4],
}

pub mod ty {
    use super::*;

    crate::utils::tagged_union! {
        primitive: PrimitiveType,
        pointer: PointerType,
    }
}

/// A representation of a type.
#[derive(Clone)]
#[repr(C)]
pub struct Type {
    /// The concrete type.
    pub ty: ty::Impl,
    /// Discriminator for [`Self::ty`].
    pub kind: u8,
    pub _reserved: [u8; 3],
}

impl Type {
    pub fn as_enum(&self) -> Option<ty::Enum> {
        self.ty.into_enum(self.kind)
    }
}

impl<T: Into<ty::Enum>> From<T> for Type {
    fn from(t: T) -> Self {
        let (kind, ty) = t.into().into_impl();
        Self {
            kind,
            ty,
            _reserved: [0; _],
        }
    }
}

impl fmt::Debug for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.ty.tagged(self.kind).fmt(f)
    }
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            kind,
            ty,
            _reserved,
        } = other;

        self.ty.tagged(self.kind) == ty.tagged(*kind) && self._reserved == *_reserved
    }
}

impl Eq for Type {}

impl std::hash::Hash for Type {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.ty.tagged(self.kind).hash(state)
    }
}

unsafe impl Pod for Header {}
unsafe impl Pod for VariableHeader {}
unsafe impl Pod for Function {}
unsafe impl Pod for FunctionVariables {}
unsafe impl Pod for File {}
unsafe impl Pod for SourceLocation {}
unsafe impl Pod for Range {}
unsafe impl Pod for Variable {}
unsafe impl Pod for RegisterLocation {}
unsafe impl Pod for FrameOffsetLocation {}
unsafe impl Pod for VariableLocationInfo {}
unsafe impl Pod for Type {}
unsafe impl Pod for PrimitiveType {}
unsafe impl Pod for PointerType {}

#[cfg(test)]
mod tests {
    use std::mem;

    use super::*;

    #[test]
    fn test_sizeof() {
        assert_eq!(mem::size_of::<Header>(), 72);
        assert_eq!(mem::align_of::<Header>(), 4);

        assert_eq!(mem::size_of::<Function>(), 16);
        assert_eq!(mem::align_of::<Function>(), 4);

        // File (version 9 format): 24 bytes with srcsrv name + srcsrv dir + revision
        assert_eq!(mem::size_of::<File>(), 24);
        assert_eq!(mem::align_of::<File>(), 4);

        assert_eq!(mem::size_of::<SourceLocation>(), 16);
        assert_eq!(mem::align_of::<SourceLocation>(), 4);

        assert_eq!(mem::size_of::<Range>(), 4);
        assert_eq!(mem::align_of::<Range>(), 4);

        assert_eq!(mem::size_of::<FunctionVariables>(), 8);
        assert_eq!(mem::align_of::<FunctionVariables>(), 4);

        assert_eq!(mem::size_of::<VariableHeader>(), 12);
        assert_eq!(mem::align_of::<VariableHeader>(), 4);

        assert_eq!(mem::size_of::<Variable>(), 20);
        assert_eq!(mem::align_of::<Variable>(), 4);

        assert_eq!(mem::size_of::<VariableLocationInfo>(), 16);
        assert_eq!(mem::align_of::<VariableLocationInfo>(), 4);

        assert_eq!(mem::size_of::<Type>(), 16);
        assert_eq!(mem::align_of::<Type>(), 4);
    }

    #[test]
    fn test_header_arch_from_bytes() {
        // Create an array of `u32`s and later transmute it to a slice of `u8`.
        // This is to get a `u8` slice which is 4-byte aligned.
        let mut nums = [0u32; size_of::<Header>() / 4];
        // There are 32B, or 8 `u32`, before `arch` in the header.
        nums[8] = 17;
        let ptr = &raw const nums as *const u8;
        let bytes: &[u8] = unsafe { std::slice::from_raw_parts(ptr, size_of::<Header>()) };

        let _arch = Header::ref_from_bytes(bytes).unwrap().arch;
    }
}
