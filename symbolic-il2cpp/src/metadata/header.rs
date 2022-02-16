use scroll::Pread;

use crate::utils::get_slice;

use super::MetadataCtx;

#[derive(Debug)]
pub struct Header<'d> {
    pub magic: u32,
    pub version: u32,
    pub string_data_buf: &'d [u8],
    pub methods_buf: &'d [u8],
    pub type_definitions_buf: &'d [u8],
    pub images_buf: &'d [u8],
    pub assemblies_buf: &'d [u8],
}

impl<'d> scroll::ctx::TryFromCtx<'d, MetadataCtx> for Header<'d> {
    type Error = scroll::Error;

    fn try_from_ctx(from: &'d [u8], _ctx: MetadataCtx) -> Result<(Self, usize), Self::Error> {
        let offset = &mut 0;

        let magic = from.gread(offset)?;
        let version = from.gread(offset)?;

        // skip: (offset + size)
        // * string literal offset
        // * string literal data
        *offset += 2 * 2 * std::mem::size_of::<u32>();

        let string_data_buf =
            get_slice(from, from.gread::<u32>(offset)?, from.gread::<u32>(offset)?)?;

        // skip: (offset + size)
        // * events
        // * properties
        *offset += 2 * 2 * std::mem::size_of::<u32>();

        let methods_offset = from.gread::<u32>(offset)?;
        dbg!(methods_offset);
        let methods_buf = get_slice(from, methods_offset, from.gread::<u32>(offset)?)?;

        // skip: (offset + size)
        // * parameter default values
        // * field default values
        // * field and parameter default values
        // * field marshaled sizes
        // * parameters
        // * fields
        // * generic parameters
        // * generic parameter constraints
        // * generic containers
        // * nested types
        // * interfaces
        // * vtable methods
        // * interface offsets
        *offset += 13 * 2 * std::mem::size_of::<u32>();

        let type_definitions_buf =
            get_slice(from, from.gread::<u32>(offset)?, from.gread::<u32>(offset)?)?;

        let images_buf = get_slice(from, from.gread::<u32>(offset)?, from.gread::<u32>(offset)?)?;

        let assemblies_buf =
            get_slice(from, from.gread::<u32>(offset)?, from.gread::<u32>(offset)?)?;

        // skip: (offset + size)
        // * field refs
        // * referenced assemblies
        // * attribute data
        // * attribute data range
        // * unresolved virtual call parameter types
        // * unresolved virtual call parameter ranges
        // * windows runtime type names
        // * windows runtime strings
        // * exported type definitions
        *offset += 9 * 2 * std::mem::size_of::<u32>();

        Ok((
            Self {
                magic,
                version,
                string_data_buf,
                methods_buf,
                type_definitions_buf,
                images_buf,
                assemblies_buf,
            },
            *offset,
        ))
    }
}

impl scroll::ctx::SizeWith<MetadataCtx> for Header<'_> {
    fn size_with(_ctx: &MetadataCtx) -> usize {
        64 * std::mem::size_of::<u32>()
    }
}
