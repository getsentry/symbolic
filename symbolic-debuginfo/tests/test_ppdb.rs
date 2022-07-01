use zerocopy::LayoutVerified;

mod raw {
    use zerocopy::FromBytes;

    pub const METADATA_SIGNATURE: u32 = 0x424A_5342;

    #[repr(C)]
    #[derive(Debug, FromBytes)]
    pub struct MetadataHeader {
        pub signature: u32,
        pub major_version: u16,
        pub minor_version: u16,
        pub _reserved: u32,
        pub version_length: u32,
    }

    #[repr(C)]
    #[derive(Debug, FromBytes)]
    pub struct MetadataHeaderPart2 {
        pub flags: u16,
        pub streams: u16,
    }

    #[repr(C)]
    #[derive(Debug, FromBytes)]
    pub struct StreamHeader {
        pub offset: u32,
        pub size: u32,
    }
}

#[derive(Debug)]
pub struct PortablePdb<'data> {
    header: &'data raw::MetadataHeader,
    version: &'data str,
    header2: &'data raw::MetadataHeaderPart2,
    buf: &'data [u8],
    streams_buf: &'data [u8],
}

impl<'data> PortablePdb<'data> {
    // TODO: make this a `Result`
    pub fn parse(buf: &'data [u8]) -> Option<Self> {
        let (lv, streams_buf) = LayoutVerified::<_, raw::MetadataHeader>::new_from_prefix(buf)?;
        let header = lv.into_ref();

        // TODO: verify signature
        // TODO: verify major/minor version
        // TODO: verify reserved

        let version_length = header.version_length as usize;
        // TODO: validate length

        let version_buf = streams_buf.get(0..version_length)?;
        let version_buf = version_buf.split(|c| *c == 0).next()?;
        let version = std::str::from_utf8(version_buf).ok()?;

        let streams_buf = streams_buf.get(version_length..)?;
        let (lv, streams_buf) =
            LayoutVerified::<_, raw::MetadataHeaderPart2>::new_from_prefix(streams_buf)?;
        let header2 = lv.into_ref();

        // TODO: validate flags

        Some(Self {
            header,
            version,
            header2,
            buf,
            streams_buf,
        })
    }

    // TODO: make this a `Result`
    pub fn streams(&self) -> impl Iterator<Item = Stream> + '_ {
        let buf = self.buf;
        let mut streams_buf = self.streams_buf;
        let mut count = self.header2.streams;
        std::iter::from_fn(move || {
            if count == 0 {
                return None;
            }
            count -= 1;

            let (lv, after_header_buf) =
                LayoutVerified::<_, raw::StreamHeader>::new_from_prefix(streams_buf)?;
            let header = lv.into_ref();

            let name_buf = after_header_buf.get(..32).unwrap_or(after_header_buf);
            let name_buf = name_buf.split(|c| *c == 0).next()?;
            let name = std::str::from_utf8(name_buf).ok()?;

            let mut rounded_name_len = name.len() + 1;
            rounded_name_len = match rounded_name_len % 4 {
                0 => rounded_name_len,
                r => rounded_name_len + (4 - r),
            };
            streams_buf = after_header_buf.get(rounded_name_len..)?;

            let offset = header.offset as usize;
            let size = header.size as usize;
            let data = buf.get(offset..offset + size)?;

            Some(Stream { name, data })
        })
    }
}

#[derive(Debug)]
pub struct Stream<'data> {
    pub name: &'data str,
    pub data: &'data [u8],
}

#[test]
fn test_ppdb() {
    let buf = std::fs::read("../EmbeddedSource.pdbx").unwrap();

    let pdb = PortablePdb::parse(&buf).unwrap();

    // dbg!(pdb);

    for stream in pdb.streams() {
        dbg!(stream.name);
    }
}
