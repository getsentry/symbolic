use scroll::Pread;

use super::MetadataCtx;

#[derive(Debug)]
pub struct MethodDefinition {
    pub name_idx: u32,
    pub declaring_type_idx: u32,
    pub return_type_idx: u32,
    pub parameter_start: u32,
    pub generic_container_idx: u32,
    pub token: u32,
    pub flags: u16,
    pub iflags: u16,
    pub slot: u16,
    pub parameter_count: u16,
}

impl scroll::ctx::TryFromCtx<'_, MetadataCtx> for MethodDefinition {
    type Error = scroll::Error;

    fn try_from_ctx(from: &[u8], _ctx: MetadataCtx) -> Result<(Self, usize), Self::Error> {
        let offset = &mut 0;

        let name_idx = from.gread(offset)?;
        let declaring_type_idx = from.gread(offset)?;
        let return_type_idx = from.gread(offset)?;
        let parameter_start = from.gread(offset)?;
        let generic_container_idx = from.gread(offset)?;
        let token = from.gread(offset)?;
        let flags = from.gread(offset)?;
        let iflags = from.gread(offset)?;
        let slot = from.gread(offset)?;
        let parameter_count = from.gread(offset)?;

        Ok((
            Self {
                name_idx,
                declaring_type_idx,
                return_type_idx,
                parameter_start,
                generic_container_idx,
                token,
                flags,
                iflags,
                slot,
                parameter_count,
            },
            *offset,
        ))
    }
}

impl scroll::ctx::SizeWith<MetadataCtx> for MethodDefinition {
    fn size_with(_ctx: &MetadataCtx) -> usize {
        6 * std::mem::size_of::<u32>() + 4 * std::mem::size_of::<u16>()
    }
}
