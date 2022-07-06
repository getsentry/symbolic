mod tables;

use std::convert::TryInto;
use std::{fmt, ptr};

use thiserror::Error;
use zerocopy::LayoutVerified;

use symbolic_common::Uuid;

use tables::TableStream;

use crate::ppdb::tables::TableType;

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

    #[repr(C, packed(4))]
    #[derive(Debug, FromBytes, Clone, Copy)]
    pub struct PdbStreamHeader {
        pub id: [u8; 20],
        pub entry_point: u32,
        pub referenced_tables: u64,
    }

    #[repr(C, packed(4))]
    #[derive(Debug, FromBytes, Clone, Copy)]
    pub struct TableStreamHeader {
        pub _reserved: u32,
        pub major_version: u8,
        pub minor_version: u8,
        pub heap_sizes: u8,
        pub _reserved2: u8,
        pub valid_tables: u64,
        pub sorted_tables: u64,
    }
}

#[derive(Debug, Clone, Copy, Error)]
pub enum ErrorKind<'data> {
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
    #[error("file does not contain a #Strings stream")]
    NoStringsStream,
    #[error("invalid string offset")]
    InvalidStringOffset,
    #[error("invalid string data")]
    InvalidStringData,
    #[error("unknown stream: {0}")]
    UnknownStream(&'data str),
    #[error("file does not contain a #Guid stream")]
    NoGuidStream,
    #[error("invalid index")]
    InvalidIndex,
    #[error(
        "insufficient table data: {0} bytes required, but table stream only contains {1} bytes"
    )]
    InsufficientTableData(usize, usize),
    #[error("invalid blob offset")]
    InvalidBlobOffset,
    #[error("invalid blob data")]
    InvalidBlobData,
    #[error("file does not contain a #Blob stream")]
    NoBlobStream,
}

#[derive(Debug, Error)]
#[error("{kind}")]
pub struct Error<'data> {
    pub(crate) kind: ErrorKind<'data>,
    #[source]
    pub(crate) source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl<'data> Error<'data> {
    /// Creates a new SymCache error from a known kind of error as well as an
    /// arbitrary error payload.
    pub(crate) fn new<E>(kind: ErrorKind<'data>, source: E) -> Self
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

impl<'data> From<ErrorKind<'data>> for Error<'data> {
    fn from(kind: ErrorKind<'data>) -> Self {
        Self { kind, source: None }
    }
}

pub struct PortablePdb<'data> {
    /// First part of the metadata header.
    header: &'data raw::MetadataHeader,
    /// The version string.
    version_string: &'data str,
    /// Second part of the metadata header.
    header2: &'data raw::MetadataHeaderPart2,
    pdb_stream: Option<PdbStream<'data>>,
    table_stream: Option<TableStream<'data>>,
    string_stream: Option<StringStream<'data>>,
    us_stream: Option<UsStream<'data>>,
    blob_stream: Option<BlobStream<'data>>,
    guid_stream: Option<GuidStream<'data>>,
}

impl fmt::Debug for PortablePdb<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PortablePdb")
            .field("header", &self.header)
            .field("version_string", &self.version_string)
            .field("header2", &self.header2)
            .field("has_pdb_stream", &self.pdb_stream.is_some())
            .field("has_table_stream", &self.table_stream.is_some())
            .field("has_string_stream", &self.string_stream.is_some())
            .field("has_us_stream", &self.us_stream.is_some())
            .field("has_blob_stream", &self.blob_stream.is_some())
            .field("has_guid_stream", &self.guid_stream.is_some())
            .finish()
    }
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
        let (lv, mut streams_buf) =
            LayoutVerified::<_, raw::MetadataHeaderPart2>::new_from_prefix(streams_buf)
                .ok_or(ErrorKind::InvalidHeader)?;
        let header2 = lv.into_ref();

        // TODO: validate flags

        let stream_count = header2.streams;

        let mut result = Self {
            header,
            version_string: version,
            header2,
            pdb_stream: None,
            table_stream: None,
            string_stream: None,
            us_stream: None,
            blob_stream: None,
            guid_stream: None,
        };

        for _ in 0..stream_count {
            let (lv, after_header_buf) =
                LayoutVerified::<_, raw::StreamHeader>::new_from_prefix(streams_buf)
                    .ok_or(ErrorKind::InvalidStreamHeader)?;
            let header = lv.into_ref();

            let name_buf = after_header_buf.get(..32).unwrap_or(after_header_buf);
            let name_buf = name_buf
                .split(|c| *c == 0)
                .next()
                .ok_or(ErrorKind::InvalidStreamName)?;
            let name = std::str::from_utf8(name_buf)
                .map_err(|e| Error::new(ErrorKind::InvalidStreamName, e))?;

            let mut rounded_name_len = name.len() + 1;
            rounded_name_len = match rounded_name_len % 4 {
                0 => rounded_name_len,
                r => rounded_name_len + (4 - r),
            };
            streams_buf = after_header_buf
                .get(rounded_name_len..)
                .ok_or(ErrorKind::InvalidLength)?;

            let offset = header.offset as usize;
            let size = header.size as usize;
            let stream_buf = buf
                .get(offset..offset + size)
                .ok_or(ErrorKind::InvalidLength)?;

            match name {
                "#Pdb" => result.pdb_stream = Some(PdbStream::parse(stream_buf)?),
                "#~" => result.table_stream = Some(TableStream::parse(stream_buf)?),
                "#Strings" => result.string_stream = Some(StringStream { buf: stream_buf }),
                "#US" => result.us_stream = Some(UsStream { buf: stream_buf }),
                "#Blob" => result.blob_stream = Some(BlobStream { buf: stream_buf }),
                "#GUID" => result.guid_stream = Some(GuidStream::parse(stream_buf)?),
                _ => return Err(ErrorKind::UnknownStream(name).into()),
            }
        }
        Ok(result)
    }

    fn get_string(&self, offset: u32) -> Result<&'data str, Error> {
        self.string_stream
            .as_ref()
            .ok_or(ErrorKind::NoStringsStream)?
            .get_string(offset)
    }

    fn get_guid(&self, idx: u32) -> Result<Uuid, Error> {
        self.guid_stream
            .as_ref()
            .ok_or(ErrorKind::NoGuidStream)?
            .get_guid(idx)
            .ok_or_else(|| ErrorKind::InvalidIndex.into())
    }

    fn get_blob(&self, offset: u32) -> Result<&'data [u8], Error> {
        self.blob_stream
            .as_ref()
            .ok_or(ErrorKind::NoBlobStream)?
            .get_blob(offset)
    }

    fn get_document_name(&self, offset: u32, idx_size: usize) -> Result<String, Error> {
        let data = self.get_blob(offset)?;
        let sep = if data[0] == 0 {
            ""
        } else {
            std::str::from_utf8(&data[..1]).unwrap()
        };

        dbg!(sep);
        dbg!(data.len());
        let mut segments = Vec::new();

        let mut data = &data[1..];
        dbg!(data);
        while !data.is_empty() {
            let (idx, rest) = decode_unsigned(data)?;
            dbg!(idx);

            let seg = if idx == 0 {
                ""
            } else {
                let seg = self.get_blob(idx)?;
                std::str::from_utf8(seg).unwrap()
            };

            data = rest;

            segments.push(seg);
        }

        Ok(segments.join(sep))
    }
}

#[derive(Debug)]
struct PdbStream<'data> {
    header: &'data raw::PdbStreamHeader,
    referenced_table_rows: &'data [u32],
}

impl<'data> PdbStream<'data> {
    fn parse(buf: &'data [u8]) -> Result<Self, Error> {
        let (lv, rest) = LayoutVerified::<_, raw::PdbStreamHeader>::new_from_prefix(buf)
            .ok_or(ErrorKind::InvalidHeader)?;
        let header = lv.into_ref();

        let num_tables = header.referenced_tables.count_ones() as usize;
        if rest.len() < num_tables * 4 {
            return Err(ErrorKind::InvalidLength.into());
        }
        let rows_start = rest.as_ptr();
        // SAFETY: We verified above that rest is long enough for num_tables u32s.
        let rows = unsafe { &*ptr::slice_from_raw_parts(rows_start as *const u32, num_tables) };

        Ok(Self {
            header,
            referenced_table_rows: rows,
        })
    }
}

#[derive(Debug)]
struct StringStream<'data> {
    buf: &'data [u8],
}

impl<'data> StringStream<'data> {
    fn get_string(&self, offset: u32) -> Result<&'data str, Error> {
        let string_buf = self
            .buf
            .get(offset as usize..)
            .ok_or(ErrorKind::InvalidStringOffset)?;
        let string = string_buf.split(|c| *c == 0).next().unwrap();
        std::str::from_utf8(string).map_err(|e| Error::new(ErrorKind::InvalidStringData, e))
    }
}

#[derive(Debug)]
struct UsStream<'data> {
    buf: &'data [u8],
}

#[derive(Debug)]
struct BlobStream<'data> {
    buf: &'data [u8],
}

impl<'data> BlobStream<'data> {
    fn get_blob(&self, offset: u32) -> Result<&'data [u8], Error> {
        let offset = offset as usize;
        let (len, rest) =
            decode_unsigned(self.buf.get(offset..).ok_or(ErrorKind::InvalidBlobOffset)?)?;

        rest.get(..len as usize)
            .ok_or_else(|| ErrorKind::InvalidBlobData.into())
    }
}

#[derive(Debug)]
struct GuidStream<'data> {
    buf: &'data [uuid::Bytes],
}

impl<'data> GuidStream<'data> {
    fn parse(buf: &'data [u8]) -> Result<Self, Error> {
        let bytes =
            LayoutVerified::<_, [uuid::Bytes]>::new_slice(buf).ok_or(ErrorKind::InvalidLength)?;

        Ok(Self {
            buf: bytes.into_slice(),
        })
    }

    fn get_guid(&self, idx: u32) -> Option<Uuid> {
        self.buf
            .get(idx.checked_sub(1)? as usize)
            .map(|bytes| Uuid::from_bytes(*bytes))
    }
}

fn decode_unsigned(mut data: &[u8]) -> Result<(u32, &[u8]), Error> {
    let first_byte = *data.first().ok_or(ErrorKind::InvalidBlobOffset)? as u32;
    data = &data[1..];
    if first_byte & (1 << 7) == 0 {
        return Ok((first_byte, data));
    }

    let second_byte = *data.first().ok_or(ErrorKind::InvalidBlobOffset)? as u32;
    data = &data[1..];

    if first_byte & (1 << 6) == 0 {
        let masked = first_byte & 0b0011_1111;
        let result = (masked << 8) + second_byte;
        return Ok((result, data));
    }

    if first_byte & (1 << 5) == 0 {
        let third_byte = *data.first().ok_or(ErrorKind::InvalidBlobOffset)? as u32;
        data = &data[1..];
        let fourth_byte = *data.first().ok_or(ErrorKind::InvalidBlobOffset)? as u32;
        data = &data[1..];

        let masked = first_byte & 0b0001_1111;
        let result = (masked << 24) + (second_byte << 16) + (third_byte << 8) + fourth_byte;
        return Ok((result, data));
    }

    Err(ErrorKind::InvalidBlobData.into())
}
#[test]
fn test_ppdb() {
    let buf = std::fs::read("_fixtures/Documents.pdbx").unwrap();

    let pdb = PortablePdb::parse(&buf).unwrap();

    let table_stream = pdb.table_stream.as_ref().unwrap();

    for (i, table) in table_stream
        .tables
        .iter()
        .enumerate()
        .filter(|(_, table)| table.rows > 0)
    {
        println!("{i:#0x}: {table:?}");
    }

    let data = table_stream.get_row(TableType::Document, 3).unwrap();
    let blob_idx = u16::from_ne_bytes(data[..2].try_into().unwrap()) as u32;
    dbg!(blob_idx);
    let file_name = pdb.get_document_name(blob_idx, 2).unwrap();
    println!("{file_name}");

    // dbg!(table_stream.get_row(TableType::LocalScope, 1));
}
