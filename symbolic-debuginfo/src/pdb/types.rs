use std::borrow::Cow;

use ms_pdb::codeview::parser::{Parser, ParserError};
use ms_pdb::tpi::{TypeStream, TypeStreamHeader};
use ms_pdb::types::{Leaf, TypeData, TypeIndex};
use ms_pdb::{BStr, Stream, StreamData};

use crate::{PointerType, PrimitiveType, PrimitiveTypeEncoding, Size, Type};

use super::{Pdb, PdbError, PdbTypeRef};

pub(super) struct Types {
    tpi: TypeStream<StreamData>,
    ipi: TypeStream<StreamData>,
}

impl Types {
    pub fn read(pdb: &Pdb<'_>) -> anyhow::Result<Self> {
        Ok(Self {
            tpi: parse_stream(Stream::TPI, pdb.read_stream(Stream::TPI.into())?)?,
            ipi: parse_stream(Stream::IPI, pdb.read_stream(Stream::IPI.into())?)?,
        })
    }

    pub fn lookup(&self, index: TypeIndex) -> Result<Option<Type<'_>>, PdbError> {
        if index >= TypeIndex::MIN_BEGIN {
            let Some(TypeData::Pointer(pointer)) = self.record(index)? else {
                return Ok(None);
            };
            let attrs = pointer.fixed.attr();
            if matches!(attrs.mode(), 2 | 3) {
                return Ok(None);
            }
            let fallback_size = match attrs.pointer_kind() {
                0 => 0,
                10 => 4,
                12 => 8,
                _ => return Ok(None),
            };
            let size = match attrs.size() {
                0 => fallback_size,
                size => size.into(),
            };
            return Ok((size != 0).then(|| {
                PointerType {
                    pointee: Some(PdbTypeRef(pointer.fixed.ty.get()).into()),
                    size: Size::Bytes(size),
                }
                .into()
            }));
        }
        if index.0 <= 0xff {
            return Ok(primitive(index.0).map(Type::Primitive));
        }
        let size = match index.0 >> 8 {
            1 => 2,
            4 => 4,
            6 => 8,
            7 => 16,
            _ => return Ok(None),
        };
        let pointee = TypeIndex(index.0 & 0xff);
        Ok(Some(
            PointerType {
                pointee: (pointee.0 != 3).then(|| PdbTypeRef(pointee).into()),
                size: Size::Bytes(size),
            }
            .into(),
        ))
    }

    fn record(&self, index: TypeIndex) -> Result<Option<TypeData<'_>>, ParserError> {
        self.tpi
            .record(index)
            .ok()
            .map(|record| record.parse())
            .transpose()
    }

    fn id(&self, index: TypeIndex) -> Option<TypeData<'_>> {
        self.ipi.record(index).ok()?.parse().ok()
    }

    fn extended_class_name(&self, index: TypeIndex) -> Option<&BStr> {
        let record = self.tpi.record(index).ok()?;
        if !matches!(record.kind, Leaf::LF_CLASS2 | Leaf::LF_STRUCTURE2) {
            return None;
        }
        let mut parser = Parser::new(record.data);
        parser.skip(18).ok()?;
        parser.number().ok()?;
        parser.strz().ok()
    }

    pub fn function_type(&self, index: TypeIndex) -> Option<TypeIndex> {
        match self.id(index)? {
            TypeData::FuncId(func) => Some(func.fixed.func_type.get()),
            TypeData::MFuncId(func) => Some(func.fixed.func_type.get()),
            _ => None,
        }
    }

    pub fn format_id(
        &self,
        index: TypeIndex,
        resolve: &impl Fn(TypeIndex) -> Option<TypeIndex>,
    ) -> Option<String> {
        self.format_scoped_id(index, 0, &mut bounded(resolve))
    }

    fn format_scoped_id(
        &self,
        index: TypeIndex,
        depth: usize,
        resolve: &mut impl FnMut(TypeIndex) -> Option<TypeIndex>,
    ) -> Option<String> {
        if depth >= 32 {
            return None;
        }
        let index = resolve(index)?;
        match self.id(index)? {
            TypeData::FuncId(func) => {
                let mut name = String::new();
                let scope = func.fixed.scope.get();
                if scope != 0 {
                    name.push_str(&self.format_scoped_id(TypeIndex(scope), depth + 1, resolve)?);
                    name.push_str("::");
                }
                name.push_str(&text(func.name));
                self.format_signature(&name, func.fixed.func_type.get(), resolve)
                    .or(Some(name))
            }
            TypeData::MFuncId(func) => {
                let class = self.format_type(func.fixed.parent_type.get(), 0, resolve)?;
                let name = format!("{class}::{}", text(func.name));
                self.format_signature(&name, func.fixed.func_type.get(), resolve)
                    .or(Some(name))
            }
            TypeData::StringId(scope) => {
                let name = text(scope.name);
                if name
                    .strip_prefix("?A0x")
                    .is_some_and(|rest| u32::from_str_radix(rest, 16).is_ok())
                {
                    Some("`anonymous namespace'".to_owned())
                } else {
                    Some(name.into_owned())
                }
            }
            _ => None,
        }
    }

    pub fn format_function(
        &self,
        name: &str,
        index: TypeIndex,
        resolve: &impl Fn(TypeIndex) -> Option<TypeIndex>,
    ) -> Option<String> {
        self.format_signature(name, index, &mut bounded(resolve))
    }

    fn format_signature(
        &self,
        name: &str,
        index: TypeIndex,
        resolve: &mut impl FnMut(TypeIndex) -> Option<TypeIndex>,
    ) -> Option<String> {
        let (args, this) = match self.record(resolve(index)?).ok()?? {
            TypeData::Proc(proc) => (proc.arg_list.get(), TypeIndex(0)),
            TypeData::MemberFunc(func) => (func.arg_list.get(), func.this.get()),
            _ => return None,
        };
        let mut result = format!("{name}({})", self.format_args(args, 0, resolve)?);
        if let Some(TypeData::Pointer(pointer)) = self.record(resolve(this)?).ok()?
            && matches!(self.record(resolve(pointer.fixed.ty.get())?).ok()?, Some(TypeData::Modifier(modifier)) if modifier.is_const())
        {
            result.push_str(" const");
        }
        Some(result)
    }

    fn format_args(
        &self,
        index: TypeIndex,
        depth: usize,
        resolve: &mut impl FnMut(TypeIndex) -> Option<TypeIndex>,
    ) -> Option<String> {
        let TypeData::ArgList(args) = self.record(resolve(index)?).ok()?? else {
            return None;
        };
        let mut result = String::new();
        for (position, arg) in args.args.iter().enumerate() {
            if position != 0 {
                result.push_str(", ");
            }
            result.push_str(&self.format_type(arg.get(), depth + 1, resolve)?);
        }
        Some(result)
    }

    fn format_type(
        &self,
        index: TypeIndex,
        depth: usize,
        resolve: &mut impl FnMut(TypeIndex) -> Option<TypeIndex>,
    ) -> Option<String> {
        if depth >= 32 {
            return None;
        }
        let index = resolve(index)?;
        if index.0 < 0x1000 {
            let kind = index.0 & 0xff;
            let name = match kind {
                0 => "<NoType>".into(),
                3 => "void".into(),
                0x13 => "long long".into(),
                0x23 => "unsigned long long".into(),
                0x72 => "int16_t".into(),
                0x73 => "uint16_t".into(),
                0x42 | 0x43 => "long double".into(),
                0x52 | 0x53 => "complex<long double>".into(),
                _ => primitive(kind)?.name?,
            };
            return Some(if index.0 > 0xff {
                format!("{name}*")
            } else {
                name.into_owned()
            });
        }
        if let Some(name) = self.extended_class_name(index) {
            return Some(text(name).into_owned());
        }
        let record = match self.record(index).ok()? {
            Some(record) => record,
            None => return Some(format!("<missing type 0x{:x}>", index.0)),
        };
        match record {
            TypeData::Modifier(modifier) => {
                let base = self.format_type(modifier.underlying_type.get(), depth + 1, resolve)?;
                Some(if modifier.is_const() {
                    format!("const {base}")
                } else {
                    base
                })
            }
            TypeData::Pointer(pointer) => {
                let attrs = pointer.fixed.attr();
                let sigil = match attrs.mode() {
                    1 => "&",
                    4 => "&&",
                    _ => "*",
                };
                let pointee = resolve(pointer.fixed.ty.get())?;
                let mut result = match self.record(pointee).ok()? {
                    Some(TypeData::Proc(proc)) => format!(
                        "{} ({sigil})({})",
                        self.format_type(proc.return_value.get(), depth + 1, resolve)?,
                        self.format_args(proc.arg_list.get(), depth + 1, resolve)?
                    ),
                    Some(TypeData::Modifier(modifier)) if modifier.is_const() => format!(
                        "{} const{sigil}",
                        self.format_type(modifier.underlying_type.get(), depth + 1, resolve)?
                    ),
                    _ => format!("{}{sigil}", self.format_type(pointee, depth + 1, resolve)?),
                };
                if attrs.r#const() {
                    result.push_str(" const");
                }
                Some(result)
            }
            other => other.name().map(|name| text(name).into_owned()),
        }
    }
}

fn parse_stream(index: Stream, data: StreamData) -> anyhow::Result<TypeStream<StreamData>> {
    if !data.is_empty() {
        let header: &TypeStreamHeader = Parser::new(&data).get()?;
        let size = header.type_record_bytes.get();
        let count = header
            .type_index_end
            .get()
            .0
            .checked_sub(header.type_index_begin.get().0);
        let end = header.header_size.get().checked_add(size);
        anyhow::ensure!(
            end.is_some_and(|end| end as usize <= data.len()),
            "invalid PDB type record range"
        );
        anyhow::ensure!(
            count.is_some_and(|count| count <= size / 4),
            "invalid PDB type record count"
        );
    }
    TypeStream::parse(index, data)
}

fn bounded(
    resolve: &impl Fn(TypeIndex) -> Option<TypeIndex>,
) -> impl FnMut(TypeIndex) -> Option<TypeIndex> + '_ {
    let mut remaining = 1024usize;
    move |index| {
        remaining = remaining.checked_sub(1)?;
        resolve(index)
    }
}

fn text(bytes: &BStr) -> Cow<'_, str> {
    String::from_utf8_lossy(bytes)
}

fn primitive(kind: u32) -> Option<PrimitiveType<'static>> {
    use PrimitiveTypeEncoding::*;
    let (name, size, encoding) = match kind {
        0x10 => ("signed char", 1, SignedChar),
        0x70 => ("char", 1, SignedChar),
        0x20 => ("unsigned char", 1, UnsignedChar),
        0x7c => ("char8_t", 1, UnsignedChar),
        0x71 => ("wchar_t", 2, UnsignedChar),
        0x7a => ("char16_t", 2, UnsignedChar),
        0x7b => ("char32_t", 4, UnsignedChar),
        0x68 => ("int8_t", 1, SignedInt),
        0x69 => ("uint8_t", 1, UnsignedInt),
        0x11 | 0x72 => ("short", 2, SignedInt),
        0x21 | 0x73 => ("unsigned short", 2, UnsignedInt),
        0x12 => ("long", 4, SignedInt),
        0x22 => ("unsigned long", 4, UnsignedInt),
        0x74 => ("int", 4, SignedInt),
        0x75 => ("unsigned int", 4, UnsignedInt),
        0x13 | 0x76 => ("int64_t", 8, SignedInt),
        0x23 | 0x77 => ("uint64_t", 8, UnsignedInt),
        0x14 | 0x78 => ("int128_t", 16, SignedInt),
        0x24 | 0x79 => ("uint128_t", 16, UnsignedInt),
        0x46 => ("float16_t", 2, Float),
        0x40 | 0x45 => ("float", 4, Float),
        0x44 => ("float48_t", 6, Float),
        0x41 => ("double", 8, Float),
        0x42 => ("float80_t", 10, Float),
        0x43 => ("float128_t", 16, Float),
        0x50 => ("complex<float>", 8, ComplexFloat),
        0x51 => ("complex<double>", 16, ComplexFloat),
        0x52 => ("complex<float80_t>", 20, ComplexFloat),
        0x53 => ("complex<float128_t>", 32, ComplexFloat),
        0x30 => ("bool", 1, Boolean),
        0x31 => ("bool16_t", 2, Boolean),
        0x32 => ("bool32_t", 4, Boolean),
        0x33 => ("bool64_t", 8, Boolean),
        0x08 => ("HRESULT", 4, SignedInt),
        _ => return None,
    };
    Some(PrimitiveType {
        name: Some(name.into()),
        encoding: Some(encoding),
        size: Size::Bytes(size),
        value_bits: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdb_type_stream_range_and_count_validation() {
        let header_size = size_of::<TypeStreamHeader>();
        let mut bytes = vec![0; header_size];
        bytes[4..8].copy_from_slice(&(header_size as u32).to_le_bytes());
        bytes[8..12].copy_from_slice(&0x1000u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&0x1000u32.to_le_bytes());
        assert!(parse_stream(Stream::TPI, StreamData::Box(bytes.clone().into())).is_ok());

        bytes[12..16].copy_from_slice(&0x1001u32.to_le_bytes());
        assert!(parse_stream(Stream::TPI, StreamData::Box(bytes.clone().into())).is_err());

        bytes[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_stream(Stream::TPI, StreamData::Box(bytes.into())).is_err());
    }
}
