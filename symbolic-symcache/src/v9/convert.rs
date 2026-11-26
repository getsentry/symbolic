use symbolic_common::CpuFamily;
use symbolic_common::register::Register;
use symbolic_debuginfo::{self as di, PrimitiveTypeEncoding, VariableKind};

use crate::v9::raw;

impl raw::Size {
    #[inline(always)]
    pub fn from_di(size: Option<di::Size>) -> Self {
        match size {
            Some(di::Size::Bytes(bytes)) if bytes < (1 << 31) => Self(bytes as u32),
            Some(di::Size::Bits(bits)) if bits < (1 << 31) => Self(1 << 31 | bits as u32),
            _ => Self(u32::MAX),
        }
    }

    #[inline(always)]
    pub fn into_di(self) -> Option<di::Size> {
        match self.0 {
            u32::MAX => None,
            v if v < 1 << 31 => Some(di::Size::Bytes(v.into())),
            v => Some(di::Size::Bits((v << 1 >> 1).into())),
        }
    }
}

impl raw::ValueBits {
    #[inline(always)]
    pub fn from_di(vb: Option<di::ValueBits>) -> Self {
        match vb {
            Some(vb) if let (Ok(size), Ok(offset)) = (vb.size.try_into(), vb.offset.try_into()) => {
                raw::ValueBits { size, offset }
            }
            Some(_) | None => raw::ValueBits {
                size: u16::MAX,
                offset: u16::MAX,
            },
        }
    }

    #[inline(always)]
    pub fn into_di(self) -> Option<di::ValueBits> {
        match (self.offset, self.size) {
            (u16::MAX, u16::MAX) => None,
            (offset, size) => Some(di::ValueBits {
                offset: offset.into(),
                size: size.into(),
            }),
        }
    }
}

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

/// Converts a [`Register`] into a `u16` to be stored in [`RegisterLocation::id`].
///
/// [`RegisterLocation::id`]: crate::v9::raw::RegisterLocation::id
pub fn register_to_u16(register: Register) -> u16 {
    register.to_dwarf()
}

/// Converts a `u16` back into a [`Register`] of the given CPU family.
///
/// The inverse of [`register_to_u16`]. Invalid inputs are returned as `None`.
pub fn u16_to_register(cpu_family: CpuFamily, id: u16) -> Option<Register> {
    Register::from_dwarf(cpu_family, id)
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;

    #[test]
    fn test_convert_size_di_to_raw() {
        const BIT_FLAG: u32 = 1 << 31;

        let conv_bytes = |s| raw::Size::from_di(Some(di::Size::Bytes(s))).0;
        let conv_bits = |s| raw::Size::from_di(Some(di::Size::Bits(s))).0;

        assert_eq!(conv_bytes(0), 0);
        assert_eq!(conv_bytes(42), 42);
        assert_eq!(conv_bytes(u64::from(BIT_FLAG)), u32::MAX);
        assert_eq!(conv_bytes(u64::from(BIT_FLAG) - 1), BIT_FLAG - 1);

        assert_eq!(conv_bits(0), BIT_FLAG);
        assert_eq!(conv_bits(42), BIT_FLAG | 42);
        assert_eq!(conv_bits(u64::from(BIT_FLAG)), u32::MAX);
        assert_eq!(
            conv_bits(u64::from(BIT_FLAG) - 1),
            BIT_FLAG | (BIT_FLAG - 1)
        );

        assert_eq!(raw::Size::from_di(None).0, u32::MAX);
    }

    #[test]
    fn test_convert_size_raw_to_di_opt() {
        const BIT_FLAG: u32 = 1 << 31;

        let conv = |s| raw::Size(s).into_di();

        assert_matches!(conv(0), Some(di::Size::Bytes(0)));
        assert_matches!(conv(42), Some(di::Size::Bytes(42)));
        assert_matches!(conv(BIT_FLAG - 1), Some(di::Size::Bytes(0x7fff_ffff)));

        assert_matches!(conv(BIT_FLAG), Some(di::Size::Bits(0)));
        assert_matches!(conv(BIT_FLAG | 42), Some(di::Size::Bits(42)));
        assert_matches!(conv(u32::MAX), None);
    }
}
