use scroll::Pread;

use crate::utils::CStrCtx;

mod header;
mod method_definition;

/// Parser Context that is being used for parsing other structs
#[derive(Clone, Copy, Debug)]
pub(crate) struct MetadataCtx {
    pub version: u32,
}

const IL2CPP_METADATA_MAGIC: u32 = 0xFAB11BAF; // TODO: use from_bytes_be

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
}
