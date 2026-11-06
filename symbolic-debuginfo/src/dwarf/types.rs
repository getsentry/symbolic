use gimli::{AttributeValue, DebuggingInformationEntry, UnitOffset, constants};

use crate::dwarf::{DwarfError, Slice, UnitRef};
use crate::{PointerType, PrimitiveType, PrimitiveTypeEncoding, Type, TypeDefType, TypeSize};

pub fn parse_type<'d>(
    unit: UnitRef<'d, '_>,
    offset: UnitOffset,
) -> Result<Option<Type<'d>>, DwarfError> {
    let entry = unit.unit.entry(offset)?;

    Ok(match entry.tag {
        constants::DW_TAG_base_type => parse_base_type(unit, entry).map(Type::Primitive),
        // Pointers may be nested within `DW_TAG_restrict_type`, `DW_TAG_const_type` etc.
        constants::DW_TAG_pointer_type
        | constants::DW_TAG_reference_type
        | constants::DW_TAG_rvalue_reference_type => {
            parse_pointer_type(unit, entry).map(Type::Pointer)
        }
        constants::DW_TAG_typedef => parse_typedef(unit, entry).map(Type::TypeDef),
        _ => None,
    })
}

fn parse_base_type<'d>(
    unit: UnitRef<'d, '_>,
    entry: DebuggingInformationEntry<Slice<'d>>,
) -> Option<PrimitiveType<'d>> {
    let mut name = None;
    let mut encoding = None;
    let mut size = None;

    for attr in entry.attrs {
        match attr.name() {
            constants::DW_AT_name => {
                // May also need to check `DW_AT_specification` and `DW_AT_abstract_origin` for the name.
                name = unit.string_value(attr.value());
            }
            // Need to handle `bit` sizes here eventually.
            constants::DW_AT_byte_size => size = attr.udata_value(),
            constants::DW_AT_encoding => {
                use AttributeValue::Encoding;
                encoding = Some(match attr.value() {
                    Encoding(constants::DW_ATE_boolean) => PrimitiveTypeEncoding::Boolean,
                    Encoding(constants::DW_ATE_address) => PrimitiveTypeEncoding::Address,
                    Encoding(constants::DW_ATE_signed) => PrimitiveTypeEncoding::SignedInt,
                    Encoding(constants::DW_ATE_unsigned) => PrimitiveTypeEncoding::UnsignedInt,
                    Encoding(constants::DW_ATE_signed_char) => PrimitiveTypeEncoding::SignedChar,
                    Encoding(constants::DW_ATE_unsigned_char) => {
                        PrimitiveTypeEncoding::UnsignedChar
                    }
                    Encoding(constants::DW_ATE_float) => PrimitiveTypeEncoding::Float,
                    Encoding(constants::DW_ATE_complex_float) => {
                        PrimitiveTypeEncoding::ComplexFloat
                    }
                    _ => continue,
                });
            }
            _ => {}
        }
    }

    Some(PrimitiveType {
        name,
        encoding,
        size: TypeSize::Bytes(size?),
    })
}

fn parse_pointer_type<'d>(
    unit: UnitRef<'d, '_>,
    entry: DebuggingInformationEntry<Slice<'d>>,
) -> Option<PointerType> {
    let mut pointee = None;
    let mut size = None;

    for attr in entry.attrs {
        match attr.name() {
            // Return `None` if `DW_AT_type` is present but cannot be resolved, rather than
            // treating the pointer as `void*`.
            constants::DW_AT_type => pointee = Some(unit.to_type_ref(attr)?),
            constants::DW_AT_byte_size => size = attr.udata_value(),
            _ => {}
        }
    }

    Some(PointerType {
        pointee: pointee.map(Into::into),
        size: TypeSize::Bytes(size.unwrap_or_else(|| unit.unit.encoding().address_size.into())),
    })
}

fn parse_typedef<'d>(
    unit: UnitRef<'d, '_>,
    entry: DebuggingInformationEntry<Slice<'d>>,
) -> Option<TypeDefType<'d>> {
    let mut name = None;
    let mut ty = None;

    for attr in entry.attrs {
        match attr.name() {
            constants::DW_AT_name => {
                name = unit.string_value(attr.value());
            }
            constants::DW_AT_type => ty = Some(unit.to_type_ref(attr)?),
            _ => {}
        }
    }

    Some(TypeDefType {
        name: name?,
        ty: ty.map(Into::into),
    })
}
