use scroll::Pread;

use super::MetadataCtx;

#[derive(Debug)]
pub struct ImageDefinition {
    pub name_idx: u32,
    pub assembly_idx: u32,
    pub first_type_idx: u32,
    pub type_count: u32,
}

impl scroll::ctx::TryFromCtx<'_, MetadataCtx> for ImageDefinition {
    type Error = scroll::Error;

    fn try_from_ctx(from: &[u8], _ctx: MetadataCtx) -> Result<(Self, usize), Self::Error> {
        let offset = &mut 0;

        let name_idx = from.gread(offset)?;
        let assembly_idx = from.gread(offset)?;
        let first_type_idx = from.gread(offset)?;
        let type_count = from.gread(offset)?;

        // skip:
        // * exported type start
        // * exported type count
        // * entry point index
        // * token
        // * custom attribute start
        // * custom attribute count
        *offset += 6 * std::mem::size_of::<u32>();

        Ok((
            Self {
                name_idx,
                assembly_idx,
                first_type_idx,
                type_count,
            },
            *offset,
        ))
    }
}

impl scroll::ctx::SizeWith<MetadataCtx> for ImageDefinition {
    fn size_with(_ctx: &MetadataCtx) -> usize {
        10 * std::mem::size_of::<u32>()
    }
}
