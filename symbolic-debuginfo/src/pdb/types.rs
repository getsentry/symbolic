use std::cell::RefCell;

use pdb_addr2line::pdb::{
    self, FallibleIterator, Indirection, ItemInformation, TypeData, TypeFinder, TypeIndex, TypeIter,
};

use crate::{PointerType, PrimitiveType, Size, Type, pdb::PdbTypeRef};

pub struct Types<'a> {
    indexer: RefCell<TypeIndexer<'a>>,
}

impl<'a> Types<'a> {
    pub fn new(info: &'a ItemInformation<'_, TypeIndex>) -> Self {
        Self {
            indexer: RefCell::new(TypeIndexer {
                finder: info.finder(),
                iter: Some(info.iter()),
            }),
        }
    }

    pub fn lookup(&self, index: TypeIndex) -> pdb::Result<Option<Type<'a>>> {
        let Some(ty) = self.find(index)? else {
            return Ok(None);
        };

        Ok(match ty {
            TypeData::Primitive(ty) if let Some(ind) = ty.indirection => {
                parse_primitive_indirection(index, ind)
            }
            TypeData::Primitive(ty) => parse_primitive(ty.kind),
            _ => None,
        })
    }

    fn find(&self, index: TypeIndex) -> pdb::Result<Option<TypeData<'a>>> {
        let item = self.indexer.borrow_mut().find(index);
        match item.and_then(|item| item.parse()) {
            Ok(data) => Ok(Some(data)),
            Err(pdb::Error::UnimplementedTypeKind(_) | pdb::Error::TypeNotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

struct TypeIndexer<'a> {
    finder: TypeFinder<'a>,
    iter: Option<TypeIter<'a>>,
}

impl<'a> TypeIndexer<'a> {
    fn find(&mut self, index: TypeIndex) -> pdb::Result<pdb::Type<'a>> {
        match self.finder.find(index) {
            Err(pdb::Error::TypeNotIndexed(..)) => {}
            result => return result,
        }

        if let Some(mut iter) = self.iter.take() {
            while let Some(item) = iter.next()? {
                self.finder.update(&iter);
                if item.index() == index {
                    self.iter = Some(iter);
                    return Ok(item);
                }
            }
        }

        Err(pdb::Error::TypeNotFound(index.0))
    }
}

fn parse_primitive(kind: pdb::PrimitiveKind) -> Option<Type<'static>> {
    use crate::PrimitiveTypeEncoding::*;
    use pdb::PrimitiveKind::*;

    let (name, size, encoding) = match kind {
        Char => ("signed char", 1, SignedChar),
        RChar => ("char", 1, SignedChar),
        UChar => ("unsigned char", 1, UnsignedChar),
        Char8 => ("char8_t", 1, UnsignedChar),
        WChar => ("wchar_t", 2, UnsignedChar),
        RChar16 => ("char16_t", 2, UnsignedChar),
        RChar32 => ("char32_t", 4, UnsignedChar),
        I8 => ("int8_t", 1, SignedInt),
        U8 => ("uint8_t", 1, UnsignedInt),
        Short | I16 => ("short", 2, SignedInt),
        UShort | U16 => ("unsigned short", 2, UnsignedInt),
        Long => ("long", 4, SignedInt),
        ULong => ("unsigned long", 4, UnsignedInt),
        I32 => ("int", 4, SignedInt),
        U32 => ("unsigned int", 4, UnsignedInt),
        Quad | I64 => ("int64_t", 8, SignedInt),
        UQuad | U64 => ("uint64_t", 8, UnsignedInt),
        Octa | I128 => ("int128_t", 16, SignedInt),
        UOcta | U128 => ("uint128_t", 16, UnsignedInt),
        F16 => ("float16_t", 2, Float),
        F32 | F32PP => ("float", 4, Float),
        F48 => ("float48_t", 6, Float),
        F64 => ("double", 8, Float),
        F80 => ("float80_t", 10, Float),
        F128 => ("float128_t", 16, Float),
        Complex32 => ("complex<float>", 8, ComplexFloat),
        Complex64 => ("complex<double>", 16, ComplexFloat),
        Complex80 => ("complex<float80_t>", 20, ComplexFloat),
        Complex128 => ("complex<float128_t>", 32, ComplexFloat),
        Bool8 => ("bool", 1, Boolean),
        Bool16 => ("bool16_t", 2, Boolean),
        Bool32 => ("bool32_t", 4, Boolean),
        Bool64 => ("bool64_t", 8, Boolean),
        HRESULT => ("HRESULT", 4, SignedInt),
        _ => return None,
    };

    let ty = PrimitiveType {
        name: Some(name.into()),
        encoding: Some(encoding),
        size: Size::Bytes(size),
        value_bits: None,
    };
    Some(ty.into())
}

fn parse_primitive_indirection(
    index: TypeIndex,
    indirection: Indirection,
) -> Option<Type<'static>> {
    let size = Size::Bytes(match indirection {
        Indirection::Near16 => 2,
        Indirection::Near32 => 4,
        Indirection::Near64 => 8,
        Indirection::Near128 => 16,
        // There is currently no support for these pointer types
        Indirection::Far16 | Indirection::Far32 | Indirection::Huge16 => return None,
    });

    // The `void` primitive kind in PDB.
    const VOID: u32 = 0x03;

    // Primitives indices are composed as follows:
    //
    // .---------------------------.------.----------.
    // |           Unused          | Mode |   Kind   |
    // '---------------------------'------'----------'
    // |+32                        |+12   |+8        |+0
    //
    // That means we can recover the primitive type by simply masking everything else away.
    //
    // See also: <https://llvm.org/docs/PDB/TpiStream.html#type-indices>.
    let pointee = TypeIndex(index.0 & 0xff);
    // Now we need to correctly handle `void*`. Which symbolic expresses as a `PointerType` with a
    // `None` pointee.
    let pointee = (pointee.0 != VOID).then_some(PdbTypeRef(pointee).into());

    Some(PointerType { pointee, size }.into())
}
