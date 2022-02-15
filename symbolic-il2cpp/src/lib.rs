use scroll::Pread;

const IL2CPP_METADATA_MAGIC: u32 = 0xFAB11BAF; // TODO: use from_bytes_be

#[derive(Debug)]
pub struct Il2CppMetadata {}

impl Il2CppMetadata {
    pub fn parse(buf: &[u8]) -> anyhow::Result<Self> {
        let offset = &mut 0;

        let magic: u32 = buf.gread(offset)?;
        if magic != IL2CPP_METADATA_MAGIC {
            anyhow::bail!("wrong file magic");
        }

        let version: u32 = buf.gread(offset)?;
        if version != 29 {
            anyhow::bail!("wrong version: expected 29, got {}", version);
        }

        Ok(Self {})
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_fn_name() {
        let fixtures_dir = PathBuf::from("../../sentry-unity-il2cpp-line-numbers/Builds");

        let metadata_path = fixtures_dir
            .join("IL2CPP.app/Contents/Resources/Data/il2cpp_data/Metadata/global-metadata.dat");
        let metadata_file = File::open(metadata_path).unwrap();
        let metadata_buf = unsafe { memmap2::Mmap::map(&metadata_file) }.unwrap();

        let metadata = Il2CppMetadata::parse(&metadata_buf).unwrap();
        dbg!(metadata);
    }
}
