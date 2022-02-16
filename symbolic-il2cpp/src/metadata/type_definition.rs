use scroll::Pread;

use super::MetadataCtx;

#[derive(Debug)]
pub struct TypeDefinition {
    pub name_idx: u32,
    pub namespace_idx: u32,
    pub declaring_type_idx: u32,
    pub parent_idx: u32,
}

impl scroll::ctx::TryFromCtx<'_, MetadataCtx> for TypeDefinition {
    type Error = scroll::Error;

    fn try_from_ctx(from: &[u8], _ctx: MetadataCtx) -> Result<(Self, usize), Self::Error> {
        let offset = &mut 0;

        let name_idx = from.gread(offset)?;
        let namespace_idx = from.gread(offset)?;

        // skip:
        // * byval type idx
        *offset += 1 * std::mem::size_of::<u32>();

        let declaring_type_idx = from.gread(offset)?;
        let parent_idx = from.gread(offset)?;

        // skip:
        // * element type idx
        // * generic container idx
        // * flags
        // * first field idx
        // * first method idx
        // * first event id
        // * first property id
        // * nested types start
        // * interfaces start
        // * vtable start
        // * interface offsets start
        *offset += 11 * std::mem::size_of::<u32>();

        // skip:
        // * method count
        // * property count
        // * field count
        // * event count
        // * nested type count
        // * vtable count
        // * interfaces count
        // * interface offsets count
        *offset += 8 * std::mem::size_of::<u16>();

        // skip:
        // * bitfield
        // * token
        *offset += 2 * std::mem::size_of::<u32>();

        Ok((
            Self {
                name_idx,
                namespace_idx,
                declaring_type_idx,
                parent_idx,
            },
            *offset,
        ))
    }
}

impl scroll::ctx::SizeWith<MetadataCtx> for TypeDefinition {
    fn size_with(_ctx: &MetadataCtx) -> usize {
        20 * std::mem::size_of::<u32>() + 8 * std::mem::size_of::<u16>()
    }
}
