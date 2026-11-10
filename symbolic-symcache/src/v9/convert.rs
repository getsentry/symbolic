use symbolic_common::CpuFamily;
use symbolic_common::register::Register;
use symbolic_debuginfo::{FrameBase, MemberOffset, PrimitiveTypeEncoding, VariableKind};

use crate::v9::raw;

const MEMBER_OFFSET_BITS_FLAG: u32 = 1 << 31;

/// Converts a [`MemberOffset`] to its packed representation, with the top bit set for bit offsets.
///
/// Returns `None` if the offset does not fit in 31 bits.
pub fn member_offset_to_raw(offset: MemberOffset) -> Option<u32> {
    let (offset, flag) = match offset {
        MemberOffset::Bytes(offset) => (offset, 0),
        MemberOffset::Bits(offset) => (offset, MEMBER_OFFSET_BITS_FLAG),
    };

    let offset = u32::try_from(offset).ok()?;
    (offset < MEMBER_OFFSET_BITS_FLAG).then_some(offset | flag)
}

/// Converts a packed offset to a [`MemberOffset`], using the top bit as the unit flag.
pub fn raw_to_member_offset(offset: u32) -> MemberOffset {
    let value = (offset & !MEMBER_OFFSET_BITS_FLAG).into();
    if offset & MEMBER_OFFSET_BITS_FLAG == 0 {
        MemberOffset::Bytes(value)
    } else {
        MemberOffset::Bits(value)
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

/// Converts a [`FrameBase`] into a `u16` to be stored in [`FunctionVariables::frame_base`].
///
/// [`FunctionVariables::frame_base`]: crate::v9::raw::FunctionVariables::frame_base
pub fn frame_base_to_u16(frame_base: Option<FrameBase>) -> u16 {
    match frame_base {
        Some(FrameBase::Register { register }) => register_to_u16(register),
        Some(FrameBase::Cfa) => raw::CFA_FRAME_BASE,
        None => raw::NO_FRAME_BASE,
    }
}

/// Converts a `u16` back into a [`FrameBase`] of the given CPU family.
///
/// The inverse of [`frame_base_to_u16`]. Unknown registers are returned as `None`.
pub fn u16_to_frame_base(cpu_family: CpuFamily, id: u16) -> Option<FrameBase> {
    match id {
        raw::NO_FRAME_BASE => None,
        raw::CFA_FRAME_BASE => Some(FrameBase::Cfa),
        id => u16_to_register(cpu_family, id).map(|register| FrameBase::Register { register }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_member_offset_out_of_range() {
        for offset in [
            1 << 31,
            u64::from(u32::MAX),
            u64::from(u32::MAX) + 1,
            u64::MAX,
        ] {
            assert_eq!(member_offset_to_raw(MemberOffset::Bytes(offset)), None);
            assert_eq!(member_offset_to_raw(MemberOffset::Bits(offset)), None);
        }
    }

    #[test]
    fn test_member_offset_roundtrip() {
        for (offset, raw) in [
            (MemberOffset::Bytes(0), 0),
            (MemberOffset::Bits(0), 0x8000_0000),
            (MemberOffset::Bytes(42), 42),
            (MemberOffset::Bits(13), 0x8000_000d),
            (MemberOffset::Bytes(0x7fff_ffff), 0x7fff_ffff),
            (MemberOffset::Bits(0x7fff_ffff), u32::MAX),
        ] {
            assert_eq!(member_offset_to_raw(offset), Some(raw));
            assert_eq!(raw_to_member_offset(raw), offset);
        }
    }
}
