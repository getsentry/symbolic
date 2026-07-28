use std::{borrow::Cow, fmt};

/// A type reference.
///
/// Links a variable to a concrete type.
#[derive(Debug, Clone)]
pub struct TypeRef(NativeTypeRef);

impl TypeRef {
    #[cfg(feature = "dwarf")]
    pub(crate) fn as_dwarf(&self) -> Option<&crate::dwarf::DwarfTypeRef> {
        match &self.0 {
            NativeTypeRef::Dwarf(dwarf) => Some(dwarf),
        }
    }
}

#[derive(Debug, Clone)]
enum NativeTypeRef {
    #[cfg(feature = "dwarf")]
    Dwarf(crate::dwarf::DwarfTypeRef),
}

#[cfg(feature = "dwarf")]
impl From<crate::dwarf::DwarfTypeRef> for TypeRef {
    fn from(value: crate::dwarf::DwarfTypeRef) -> Self {
        Self(NativeTypeRef::Dwarf(value))
    }
}

/// A concrete type.
#[derive(Debug, Clone)]
pub enum Type<'data> {
    /// A primitive type.
    Primitive(PrimitiveType<'data>),
    /// A pointer type.
    Pointer(PointerType),
}

/// A primitive type.
///
/// A primitive type does not link to other types and contains a concrete value, this class contains
/// integers, floats, booleans, chars, etc.
#[derive(Debug, Clone)]
pub struct PrimitiveType<'data> {
    /// The name of the type.
    ///
    /// In rare cases the name may not be available.
    pub name: Option<Cow<'data, str>>,
    /// An optional encoding of the type.
    ///
    /// The encoding gives additional information how the value of the type is to be interpreted.
    pub encoding: Option<PrimitiveEncoding>,
    /// The size of the primitive type.
    pub size: TypeSize,
}

/// An encoding for a [`PrimitiveType`].
///
/// The encoding provides supplementary information how to interpret the value of a primitive type.
#[derive(Debug, Clone)]
pub enum PrimitiveEncoding {
    /// The value is a boolean.
    Boolean,
    /// The value is an address.
    Address,
    /// The value is a signed integer.
    SignedInt,
    /// The value is an un-signed integer.
    UnsignedInt,
    /// The value is a signed char.
    SignedChar,
    /// The value is a un-signed char.
    UnsignedChar,
    /// The value is a float.
    Float,
    /// The value is a complex float.
    ComplexFloat,
}

/// A pointer or reference type.
///
/// This type always links to another type via a memory address.
#[derive(Debug, Clone)]
pub struct PointerType {
    /// The type the pointer references.
    pub pointee: TypeRef,
    /// The size of a pointer.
    pub size: TypeSize,
}

/// The size of a type in memory.
#[derive(Debug, Clone)]
pub enum TypeSize {
    /// The size is given in bytes.
    Bytes(u64),
}

/// A single variable available in a function scope.
#[derive(Debug, Clone)]
pub struct Variable<'data> {
    /// The name of the variable.
    pub name: Cow<'data, str>,
    /// The type of the variable.
    ///
    /// May be `None` if the variable had no type information attached or it could not be parsed.
    pub ty: Option<TypeRef>,
    /// The kind of the variable.
    pub kind: Kind,
    /// Possible locations at runtime of the variable.
    ///
    /// Locations are stored in ascending order based on their [`LocationInfo::address`].
    ///
    /// There may be multiple overlapping locations for the same pc range, if the variable
    /// can be sourced from multiple locations.
    pub locations: Vec<LocationInfo>,
}

/// The variable kind.
#[derive(Debug, Copy, Clone)]
pub enum Kind {
    /// The variable is a function parameter.
    Parameter,
    /// The variable is a local.
    Local,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parameter => f.write_str("parameter"),
            Self::Local => f.write_str("local"),
        }
    }
}

/// Contains metadata describing the location of a variable at runtime.
#[derive(Clone)]
pub struct LocationInfo {
    /// Start of the address range of this location's validity.
    pub address: u64,
    /// Size of the range marking the end of the location's validity.
    pub size: u64,
    /// The location of the variable at runtime.
    pub location: Location,
}

impl fmt::Debug for LocationInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocationInfo")
            .field("address", &format_args!("{:#x}", self.address))
            .field("size", &format_args!("{:#x}", self.size))
            .field("location", &self.location)
            .finish()
    }
}

/// Describes the location of a variable at runtime.
#[derive(Debug, Clone)]
pub enum Location {
    /// The variable can be found in a register.
    Register {
        /// An architecture dependent id of the register.
        id: u16,
    },
    /// The variable can be found at an offset relative to the function's frame base.
    FrameOffset {
        /// The signed offset from the frame base.
        offset: i64,
    },
}
