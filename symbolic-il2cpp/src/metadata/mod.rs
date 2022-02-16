use std::collections::HashMap;

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

    pub fn build_method_map(self) -> Result<HashMap<String, HashMap<u32, String>>, scroll::Error> {
        let mut method_printer = MethodPrinter {
            metadata: &self,
            typedef_map: HashMap::new(),
        };

        let mut method_map = HashMap::new();

        // iterate over images
        let mut images_buf = self.header.images_buf;
        while !images_buf.is_empty() {
            let offset = &mut 0;

            let image: ImageDefinition = images_buf.gread_with(offset, self.ctx)?;
            let image_name = self.get_str_at_idx(image.name_idx)?;

            let mut indexed_methods = HashMap::new();

            if image_name == "Assembly-CSharp.dll" {
                dbg!(image_name, &image,);

                // iterate over types inside the image
                let mut types_buf = sub_slice::<TypeDefinition, MetadataCtx>(
                    self.header.type_definitions_buf,
                    &self.ctx,
                    image.first_type_idx,
                    image.type_count,
                )?;
                while !types_buf.is_empty() {
                    let offset = &mut 0;

                    let typedef: TypeDefinition = types_buf.gread_with(offset, self.ctx)?;
                    dbg!(self.get_str_at_idx(typedef.name_idx)?, &typedef,);

                    // iterate over the methods of the types
                    if typedef.method_count > 0 && typedef.first_method_idx < u32::MAX {
                        let mut methods_buf = sub_slice::<MethodDefinition, MetadataCtx>(
                            self.header.methods_buf,
                            &self.ctx,
                            typedef.first_method_idx,
                            typedef.method_count as u32,
                        )?;

                        while !methods_buf.is_empty() {
                            let offset = &mut 0;

                            let method: MethodDefinition =
                                methods_buf.gread_with(offset, self.ctx)?;
                            let assembly_methods_idx = method.token & 0x00FF_FFFF;
                            if assembly_methods_idx > 0 {
                                // TODO: pretty print, insert into map
                            }
                            dbg!(
                                self.get_str_at_idx(method.name_idx),
                                &method,
                                assembly_methods_idx
                            );

                            methods_buf = &methods_buf[*offset..];
                        }
                    }

                    types_buf = &types_buf[*offset..];
                }
            }

            if !indexed_methods.is_empty() {
                method_map.insert(image_name.to_string(), indexed_methods);
            }

            images_buf = &images_buf[*offset..];
        }

        Ok(method_map)
    }
}

struct MethodPrinter<'d> {
    metadata: &'d Il2CppMetadata<'d>,
    typedef_map: HashMap<u32, String>,
}

impl MethodPrinter<'_> {
    // fn pretty_print_type(&mut self, ty_idx: u32) -> Result<&str, scroll::Error> {}
    pub fn pretty_print_method(
        &mut self,
        method: &MethodDefinition,
    ) -> Result<String, scroll::Error> {
        // let method_name =
        Ok(self.metadata.get_str_at_idx(method.name_idx)?.to_string())
    }
}
