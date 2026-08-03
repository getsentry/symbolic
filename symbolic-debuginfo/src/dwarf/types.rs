use gimli::{AttributeValue, DebuggingInformationEntry, UnitOffset, constants};

use crate::dwarf::{DwarfError, Slice, UnitRef};
use crate::{PointerType, PrimitiveTypeEncoding, PrimitiveType, Type, TypeRef, TypeSize};

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
                    Encoding(constants::DW_ATE_unsigned_char) => PrimitiveTypeEncoding::UnsignedChar,
                    Encoding(constants::DW_ATE_float) => PrimitiveTypeEncoding::Float,
                    Encoding(constants::DW_ATE_complex_float) => PrimitiveTypeEncoding::ComplexFloat,
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
            constants::DW_AT_type => pointee = unit.to_type_ref(attr),
            constants::DW_AT_byte_size => size = attr.udata_value(),
            _ => {}
        }
    }

    Some(PointerType {
        pointee: pointee.map(TypeRef::from)?,
        size: TypeSize::Bytes(size.unwrap_or_else(|| unit.unit.encoding().address_size.into())),
    })
}
