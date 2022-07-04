use thiserror::Error;
use zerocopy::LayoutVerified;

use self::raw::StreamHeader;

mod raw {
    use zerocopy::FromBytes;

    /// Signature for physical metadata as specified by ECMA-335.
    pub const METADATA_SIGNATURE: u32 = 0x424A_5342;

    /// First part of the metadata header, as specified in the ECMA-335 spec, II.24.2.1.
    ///
    /// This includes everything before the version string.
    #[repr(C)]
    #[derive(Debug, FromBytes)]
    pub struct MetadataHeader {
        /// The metadata signature.
        ///
        /// The value of this should be [`METADATA_SIGNATURE`].
        pub signature: u32,
        /// Major version, 1 (ignore on read).
        pub major_version: u16,
        /// Minor version, 1 (ignore on read).
        pub minor_version: u16,
        /// Reserved, always 0.
        pub _reserved: u32,
        /// Number of bytes allocated to hold version string.
        ///
        /// This is the actual length of the version string, including the
        /// null terminator, rounded up to a multiple of 4.
        pub version_length: u32,
    }

    /// Second part of the metadata header, as specified in the ECMA-335 spec, II.24.2.1.
    ///
    /// This includes everything after the version string.
    #[repr(C)]
    #[derive(Debug, FromBytes)]
    pub struct MetadataHeaderPart2 {
        /// Reserved, always 0.
        pub flags: u16,
        /// Number of streams.
        pub streams: u16,
    }

    /// A stream header, as specified in the ECMA-335 spec, II.24.2.2.
    ///
    /// Does not contain the stream's name due to its variable length.
    #[repr(C)]
    #[derive(Debug, FromBytes)]
    pub struct StreamHeader {
        /// Memory offset to start of this stream form start of the metadata root.
        pub offset: u32,
        /// Size of this stream in bytes.
        ///
        /// This should always be a multiple of 4.
        pub size: u32,
    }
}

#[derive(Debug, Clone, Copy, Error)]
pub enum ErrorKind {
    #[error("invalid header")]
    InvalidHeader,
    #[error("invalid signature")]
    InvalidSignature,
    #[error("invalid length")]
    InvalidLength,
    #[error("invalid version string")]
    InvalidVersionString,
    #[error("invalid stream header")]
    InvalidStreamHeader,
    #[error("invalid stream name")]
    InvalidStreamName,
}

#[derive(Debug, Error)]
#[error("{kind}")]
pub struct Error {
    pub(crate) kind: ErrorKind,
    #[source]
    pub(crate) source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl Error {
    /// Creates a new SymCache error from a known kind of error as well as an
    /// arbitrary error payload.
    pub(crate) fn new<E>(kind: ErrorKind, source: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let source = Some(source.into());
        Self { kind, source }
    }

    /// Returns the corresponding [`ErrorKind`] for this error.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self { kind, source: None }
    }
}

#[derive(Debug)]
pub struct PortablePdb<'data> {
    /// First part of the metadata header.
    header: &'data raw::MetadataHeader,
    /// The version string.
    version: &'data str,
    /// Second part of the metadata header.
    header2: &'data raw::MetadataHeaderPart2,
    buf: &'data [u8],
    streams_buf: &'data [u8],
}

impl<'data> PortablePdb<'data> {
    pub fn parse(buf: &'data [u8]) -> Result<Self, Error> {
        let (lv, rest) = LayoutVerified::<_, raw::MetadataHeader>::new_from_prefix(buf)
            .ok_or(ErrorKind::InvalidHeader)?;
        let header = lv.into_ref();

        if header.signature != raw::METADATA_SIGNATURE {
            return Err(ErrorKind::InvalidSignature.into());
        }

        // TODO: verify major/minor version
        // TODO: verify reserved
        let version_length = header.version_length as usize;
        let version_buf = rest.get(..version_length).ok_or(ErrorKind::InvalidLength)?;
        let version_buf = version_buf
            .split(|c| *c == 0)
            .next()
            .ok_or(ErrorKind::InvalidVersionString)?;
        let version = std::str::from_utf8(version_buf)
            .map_err(|e| Error::new(ErrorKind::InvalidVersionString, e))?;

        // We already know that buf is long enough.
        let streams_buf = &rest[version_length..];
        let (lv, streams_buf) =
            LayoutVerified::<_, raw::MetadataHeaderPart2>::new_from_prefix(streams_buf)
                .ok_or(ErrorKind::InvalidHeader)?;
        let header2 = lv.into_ref();

        // TODO: validate flags

        Ok(Self {
            header,
            version,
            header2,
            buf,
            streams_buf,
        })
    }

    fn stream_headers(
        &self,
    ) -> impl Iterator<Item = Result<(&'data str, &'data raw::StreamHeader), Error>> + '_ {
        let mut streams_buf = self.streams_buf;
        let mut count = self.header2.streams;
        std::iter::from_fn(move || {
            if count == 0 {
                return None;
            }
            count -= 1;

            let (lv, after_header_buf) =
                match LayoutVerified::<_, raw::StreamHeader>::new_from_prefix(streams_buf) {
                    Some((lv, after_header_buf)) => (lv, after_header_buf),
                    None => return Some(Err(ErrorKind::InvalidStreamHeader.into())),
                };
            let header = lv.into_ref();

            let name_buf = after_header_buf.get(..32).unwrap_or(after_header_buf);
            let name_buf = match name_buf.split(|c| *c == 0).next() {
                Some(name_buf) => name_buf,
                None => return Some(Err(ErrorKind::InvalidStreamName.into())),
            };
            let name = match std::str::from_utf8(name_buf) {
                Ok(name) => name,
                Err(e) => return Some(Err(Error::new(ErrorKind::InvalidStreamName, e))),
            };

            let mut rounded_name_len = name.len() + 1;
            rounded_name_len = match rounded_name_len % 4 {
                0 => rounded_name_len,
                r => rounded_name_len + (4 - r),
            };
            streams_buf = match after_header_buf.get(rounded_name_len..) {
                Some(streams_buf) => streams_buf,
                None => return Some(Err(ErrorKind::InvalidLength.into())),
            };

            Some(Ok((name, header)))
        })
    }

    fn get_stream(&self, name: &'data str, header: &StreamHeader) -> Result<Stream, Error> {
        let offset = header.offset as usize;
        let size = header.size as usize;
        let data = match self.buf.get(offset..offset + size) {
            Some(data) => data,
            None => return Err(ErrorKind::InvalidLength.into()),
        };

        Ok(Stream { name, data })
    }

    pub fn streams(&self) -> impl Iterator<Item = Result<Stream, Error>> + '_ {
        self.stream_headers()
            .map(move |hdr| hdr.and_then(|(name, header)| self.get_stream(name, header)))
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
        dbg!(stream.unwrap().name);
    }

    assert!(false);
}
