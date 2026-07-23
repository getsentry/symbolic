use std::borrow::Cow;

///
#[derive(Debug, Clone)]
pub struct TypeRef(NativeTypeRef);

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

#[derive(Debug, Clone)]
pub struct Variable<'data> {
    /// The name of the variable.
    pub name: Cow<'data, str>,
    /// The type of the variable.
    ///
    /// May be `None` if the variable had no type information attached or could not be parsed.
    pub ty: Option<TypeRef>,
    /// The kind of the variable.
    pub kind: Kind,
    /// Possible locations at runtime of the variable.
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

/// A half open address range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Range {
    /// The beginning address of the range.
    pub begin: u64,
    /// The first address past the end of the range.
    pub end: u64,
}

/// Contains metadata describing the location of a variable at runtime.
#[derive(Debug, Clone)]
pub struct LocationInfo {
    /// Start of the address range of this location's validity.
    pub address: u64,
    /// Size of the range marking the end of the location's validity.
    pub size: u64,
    /// The location of the variable at runtime.
    pub location: Location,
}

/// Describes the location of a variable at runtime.
#[derive(Debug, Clone)]
pub enum Location {
    /// The variable can be found in a register.
    Register {
        /// An architecture dependent id of the register.
        id: u16,
    },
}
