use symbolic_debuginfo::{PrimitiveTypeEncoding, VariableKind};

/// Converts a `u8` back into a [`PrimitiveTypeEncoding`].
///
/// The inverse of [`primitive_type_encoding_to_u8`]. Invalid inputs are returned as `None`.
pub fn u8_to_primitive_type_encoding(encoding: u8) -> Option<PrimitiveTypeEncoding> {
    match encoding {
        0 => Some(PrimitiveTypeEncoding::Boolean),
        1 => Some(PrimitiveTypeEncoding::Address),
        2 => Some(PrimitiveTypeEncoding::SignedInt),
        3 => Some(PrimitiveTypeEncoding::UnsignedInt),
        4 => Some(PrimitiveTypeEncoding::SignedChar),
        5 => Some(PrimitiveTypeEncoding::UnsignedChar),
        6 => Some(PrimitiveTypeEncoding::Float),
        7 => Some(PrimitiveTypeEncoding::ComplexFloat),
        u8::MAX => None,
        _ => None,
    }
}

/// Converts an optional [`PrimitiveTypeEncoding`] into a `u8`.
pub fn primitive_type_encoding_to_u8(encoding: Option<PrimitiveTypeEncoding>) -> u8 {
    match encoding {
        Some(PrimitiveTypeEncoding::Boolean) => 0,
        Some(PrimitiveTypeEncoding::Address) => 1,
        Some(PrimitiveTypeEncoding::SignedInt) => 2,
        Some(PrimitiveTypeEncoding::UnsignedInt) => 3,
        Some(PrimitiveTypeEncoding::SignedChar) => 4,
        Some(PrimitiveTypeEncoding::UnsignedChar) => 5,
        Some(PrimitiveTypeEncoding::Float) => 6,
        Some(PrimitiveTypeEncoding::ComplexFloat) => 7,
        None => u8::MAX,
    }
}

/// Converts a `u8` back into a [`VariableKind`].
///
/// The input is expected to be a tag produced by [`variable_kind_to_u8`]. If the kind is unknown
/// the implementation falls back to [`VariableKind::Local`].
pub fn u8_to_variable_kind(kind: u8) -> VariableKind {
    match kind {
        0 => VariableKind::Parameter,
        1 => VariableKind::Local,
        // A variable kind is not optional, an unknown value is either due to a symcache version mismatch,
        // a corrupted cache or a bug.
        _ => {
            debug_assert!(false, "invalid variable kind");
            VariableKind::Local
        }
    }
}

/// Converts a [`VariableKind`] to a `u8` to be stored in [`Variable::kind`].
///
/// [`Variable::kind`]: crate::v9::raw::Variable::kind
pub fn variable_kind_to_u8(kind: VariableKind) -> u8 {
    match kind {
        VariableKind::Parameter => 0,
        VariableKind::Local => 1,
    }
}
