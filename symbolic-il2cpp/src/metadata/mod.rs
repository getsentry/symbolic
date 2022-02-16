use scroll::ctx::SizeWith;
use scroll::Pread;

use crate::utils::{sub_slice, CStrCtx};

use self::image_definition::ImageDefinition;
use self::method_definition::MethodDefinition;
use self::type_definition::TypeDefinition;

mod header;
mod image_definition;
mod method_definition;
mod type_definition;

/// Parser Context that is being used for parsing other structs
#[derive(Clone, Copy, Debug)]
pub(crate) struct MetadataCtx {
    pub version: u32,
}

const IL2CPP_METADATA_MAGIC: u32 = 0xFAB1_1BAF; // TODO: use from_bytes_be

#[derive(Debug)]
pub struct Il2CppMetadata<'d> {
    data: &'d [u8],
    ctx: MetadataCtx,
    header: header::Header<'d>,
}

impl<'d> Il2CppMetadata<'d> {
    pub fn parse(data: &'d [u8]) -> anyhow::Result<Self> {
        let offset = &mut 0;

        let magic: u32 = data.gread(offset)?;
        if magic != IL2CPP_METADATA_MAGIC {
            anyhow::bail!("wrong file magic");
        }

        let version: u32 = data.gread(offset)?;
        if version != 29 {
            anyhow::bail!("wrong version: expected 29, got {}", version);
        }

        *offset = 0;

        let ctx = MetadataCtx { version };

        let header = data.gread_with(offset, ctx)?;
        Ok(Self { data, ctx, header })
    }

    fn get_str_at_idx(&self, idx: u32) -> Result<&str, scroll::Error> {
        self.header
            .string_data_buf
            .pread_with(idx as usize, CStrCtx)
    }

    pub fn build_method_map(self) -> Result<(), scroll::Error> {
        let mut images_buf = self.header.images_buf;
        while !images_buf.is_empty() {
            let offset = &mut 0;

            let image: ImageDefinition = images_buf.gread_with(offset, self.ctx)?;
            let image_name = self.get_str_at_idx(image.name_idx)?;
            dbg!(&image, image_name);

            if image_name == "Assembly-CSharp.dll" {
                let mut types_buf = sub_slice::<TypeDefinition, MetadataCtx>(
                    self.header.type_definitions_buf,
                    &self.ctx,
                    image.first_type_idx,
                    image.type_count,
                )?;
                while !types_buf.is_empty() {
                    let offset = &mut 0;

                    let typedef: TypeDefinition = types_buf.gread_with(offset, self.ctx)?;
                    dbg!(&typedef, self.get_str_at_idx(typedef.name_idx)?);

                    types_buf = &types_buf[*offset..];
                }
            }

            images_buf = &images_buf[*offset..];
        }

        // let mut methods_buf = self.header.methods_buf;
        // while !methods_buf.is_empty() {
        //     let offset = &mut 0;

        //     let method: MethodDefinition = methods_buf.gread_with(offset, self.ctx)?;
        //     let assembly_methods_idx = method.token & 0x00FF_FFFF;
        //     if assembly_methods_idx > 0 {
        //         dbg!(&method, self.get_str_at_idx(method.name_idx));
        //     }

        //     methods_buf = &methods_buf[*offset..];
        // }

        Ok(())
    }
}
