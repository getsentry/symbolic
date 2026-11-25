use std::cell::RefCell;
use std::collections::{HashMap, HashSet, hash_map::Entry};

use pdb_addr2line::pdb::{
    self, FallibleIterator, Indirection, ItemInformation, TypeData, TypeFinder, TypeIndex, TypeIter,
};

use crate::pdb::PdbTypeRef;
use crate::{PointerType, PrimitiveType, Size, StructureMember, StructureType, Type, TypeDefType};

pub struct Types<'a> {
    indexer: RefCell<TypeIndexer<'a>>,
}

impl<'a> Types<'a> {
    pub fn new(info: &'a ItemInformation<'_, TypeIndex>) -> Self {
        Self {
            indexer: RefCell::new(TypeIndexer {
                finder: info.finder(),
                iter: Some(info.iter()),
                class_definitions: HashMap::new(),
            }),
        }
    }

    pub fn lookup(&self, index: TypeIndex) -> pdb::Result<Option<Type<'a>>> {
        let Some((kind, ty)) = self.find(index)? else {
            return Ok(None);
        };

        Ok(match ty {
            TypeData::Primitive(ty) if let Some(ind) = ty.indirection => {
                parse_primitive_indirection(index, ind)
            }
            TypeData::Primitive(ty) => parse_primitive(ty.kind),
            TypeData::Pointer(ty) => parse_pointer(ty),
            TypeData::Alias(ty) => Some(parse_alias(ty)),
            TypeData::Class(ty) => self.parse_class(kind, ty)?.map(Into::into),
            _ => None,
        })
    }

    fn find(&self, index: TypeIndex) -> pdb::Result<Option<(u16, TypeData<'a>)>> {
        let item = self.indexer.borrow_mut().find(index);
        match item.and_then(|item| {
            let data = item.parse()?;
            Ok((item.raw_kind(), data))
        }) {
            Ok(data) => Ok(Some(data)),
            Err(pdb::Error::UnimplementedTypeKind(_) | pdb::Error::TypeNotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn resolve_forward_reference(
        &self,
        kind: u16,
        ty: pdb::ClassType<'a>,
    ) -> pdb::Result<Option<pdb::ClassType<'a>>> {
        let Some(name) = ty.unique_name.filter(|name| !name.is_empty()) else {
            return Ok(None);
        };

        let class_def = {
            let mut indexer = self.indexer.borrow_mut();
            indexer.find_class_definition(kind, name)?
        };

        Ok(class_def
            .and_then(|x| self.find(x).transpose())
            .transpose()?
            .and_then(|(_, td)| match td {
                TypeData::Class(ty) => Some(ty),
                _ => None,
            }))
    }

    fn parse_class(
        &self,
        kind: u16,
        mut ty: pdb::ClassType<'a>,
    ) -> pdb::Result<Option<StructureType<'a>>> {
        if ty.kind == pdb::ClassKind::Interface {
            return Ok(None);
        }

        if ty.properties.forward_reference() {
            ty = match self.resolve_forward_reference(kind, ty)? {
                Some(ty) => ty,
                None => return Ok(None),
            };
        }

        let name = |name: pdb::RawString<'a>| match name.is_empty() {
            false => Some(name.to_string()),
            true => None,
        };

        let mut members = Vec::new();
        let mut seen = HashSet::new();

        let mut fields = ty.fields;
        while let Some(index) = fields {
            // Prevent cycles via the `continuation` field.
            //
            // This is either malformed or malicious.
            if !seen.insert(index) {
                return Ok(None);
            }

            let Some((_, TypeData::FieldList(list))) = self.find(index)? else {
                return Ok(None);
            };

            let members_from_list = list
                .fields
                .into_iter()
                .filter_map(|f| match f {
                    TypeData::Member(member) => Some(member),
                    _ => None,
                })
                .map(|member| StructureMember {
                    name: name(member.name),
                    ty: PdbTypeRef(member.field_type).into(),
                    offset: Some(Size::Bytes(member.offset)),
                    size: None,
                });

            members.extend(members_from_list);

            fields = list.continuation;
        }

        Ok(Some(StructureType {
            name: name(ty.name),
            size: Some(Size::Bytes(ty.size)),
            members,
        }))
    }
}

/// A unique identifier made up from a [`pdb::Type::raw_kind`] and [`pdb::ClassType::unique_name`].
///
/// Used to store and resolve forward references of classes.
type ClassKey<'a> = (u16, pdb::RawString<'a>);

struct NextType<'a> {
    ty: pdb::Type<'a>,
    class_def: Option<ClassKey<'a>>,
}

struct TypeIndexer<'a> {
    finder: TypeFinder<'a>,
    iter: Option<TypeIter<'a>>,
    class_definitions: HashMap<ClassKey<'a>, TypeIndex>,
}

impl<'a> TypeIndexer<'a> {
    fn find(&mut self, index: TypeIndex) -> pdb::Result<pdb::Type<'a>> {
        match self.finder.find(index) {
            Err(pdb::Error::TypeNotIndexed(..)) => {}
            result => return result,
        }

        while let Some(NextType { ty, .. }) = self.next_type()? {
            if ty.index() == index {
                return Ok(ty);
            }
        }

        Err(pdb::Error::TypeNotFound(index.0))
    }

    fn find_class_definition(
        &mut self,
        kind: u16,
        name: pdb::RawString<'a>,
    ) -> pdb::Result<Option<TypeIndex>> {
        let key = (kind, name);
        if let Some(&index) = self.class_definitions.get(&key) {
            return Ok(Some(index));
        }

        // The class definition isn't resolved yet, so it must be in one of the following types we
        // have yet to consume.
        while let Some(NextType { ty, class_def }) = self.next_type()? {
            if class_def == Some(key) {
                return Ok(Some(ty.index()));
            }
        }

        // Or it doesn't exist at all.
        Ok(None)
    }

    fn next_type(&mut self) -> pdb::Result<Option<NextType<'a>>> {
        let Some(iter) = &mut self.iter else {
            return Ok(None);
        };

        match iter.next() {
            Ok(Some(ty)) => {
                self.finder.update(iter);
                let class_def = self.index_class_definition(&ty);
                Ok(Some(NextType { ty, class_def }))
            }
            Ok(None) => {
                self.iter = None;
                Ok(None)
            }
            Err(error) => {
                self.iter = None;
                Err(error)
            }
        }
    }

    fn index_class_definition(&mut self, item: &pdb::Type<'a>) -> Option<ClassKey<'a>> {
        const LF_CLASS: u16 = 0x1504;
        const LF_CLASS_ST: u16 = 0x1004;
        const LF_CLASS2: u16 = 0x1608;
        const LF_STRUCTURE: u16 = 0x1505;
        const LF_STRUCTURE_ST: u16 = 0x1005;
        const LF_STRUCTURE2: u16 = 0x1609;

        let kind = item.raw_kind();

        // A fast check, so we don't have to parse every type.
        let is_class = matches!(
            kind,
            LF_CLASS | LF_CLASS_ST | LF_CLASS2 | LF_STRUCTURE | LF_STRUCTURE_ST | LF_STRUCTURE2
        );

        if is_class
            && let Ok(TypeData::Class(class)) = item.parse()
            && !class.properties.forward_reference()
            && let Some(name) = class.unique_name.filter(|name| !name.is_empty())
        {
            // The same forward reference can be resolved multiple times when different compilation
            // units emit debug information for the same type.
            //
            // In all cases they should describe the same layout of the underlying type, so we
            // follow what LLVM does and use the first match for the forward reference we find.
            let key = (kind, name);
            if let Entry::Vacant(entry) = self.class_definitions.entry(key) {
                entry.insert(item.index());
                return Some(key);
            }
        }

        None
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

fn parse_pointer(ty: pdb::PointerType) -> Option<Type<'static>> {
    // This is the equivalent to `DW_TAG_ptr_to_member_type` which is also currently not supported.
    if ty.attributes.pointer_to_member() {
        return None;
    }

    use pdb::PointerKind::*;
    if !matches!(ty.attributes.pointer_kind(), Near16 | Near32 | Ptr64) {
        return None;
    }

    // This does not handle `void* pointers correctly currently. This will be addressed once void
    // pointers handled as dedicated types instead of through `Option::None`.
    let pointee = Some(PdbTypeRef(ty.underlying_type).into());
    let size = ty.attributes.size();
    if size == 0 {
        return None;
    }
    let size = Size::Bytes(size.into());

    Some(PointerType { pointee, size }.into())
}

fn parse_alias<'a>(ty: pdb::AliasType<'a>) -> Type<'a> {
    let name = ty.name.to_string();
    // Same as for `parse_pointer`, void types are currently not handled.
    let ty = Some(PdbTypeRef(ty.underlying_type).into());
    TypeDefType { name, ty }.into()
}
