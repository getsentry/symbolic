mod lookup;
mod tables;

use std::convert::TryInto;
use std::fmt;

use thiserror::Error;
use zerocopy::LayoutVerified;

use symbolic_common::Uuid;

use tables::{TableStream, TableType};

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
    #[error("file does not contain a #Strings stream")]
    NoStringsStream,
    #[error("invalid string offset")]
    InvalidStringOffset,
    #[error("invalid string data")]
    InvalidStringData,
    #[error("unknown stream")]
    UnknownStream,
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
    #[error("invalid compressed unsigned number")]
    InvalidCompressedUnsigned,
    #[error("invalid compressed signed number")]
    InvalidCompressedSigned,
    #[error("invalid document name")]
    InvalidDocumentName,
    #[error("invalid sequence point")]
    InvalidSequencePoint,
    #[error("file does not contain a #~ stream")]
    NoMetadataStream,
    #[error("row index {1} is out of bounds for table {0:?}")]
    RowIndexOutOfBounds(TableType, usize),
    #[error("column index {1} is out of bounds for table {0:?}")]
    ColIndexOutOfBounds(TableType, usize),
    #[error("column {1} in table {0:?} has incompatible with {2}")]
    ColumnWidth(TableType, usize, usize),
    #[error("il offset {0} not covered by sequence points")]
    IlOffsetNotCovered(u32),
    #[error("no sequence point information for method {0}")]
    NoSequencePoints(usize),
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

#[derive(Clone)]
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
                "#~" => {
                    result.table_stream = Some(TableStream::parse(
                        stream_buf,
                        result
                            .pdb_stream
                            .as_ref()
                            .map_or([0; 64], |s| s.referenced_table_sizes),
                    )?)
                }
                "#Strings" => result.string_stream = Some(StringStream { buf: stream_buf }),
                "#US" => result.us_stream = Some(UsStream { buf: stream_buf }),
                "#Blob" => result.blob_stream = Some(BlobStream { buf: stream_buf }),
                "#GUID" => result.guid_stream = Some(GuidStream::parse(stream_buf)?),
                _ => return Err(ErrorKind::UnknownStream.into()),
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

    fn get_sequence_points(
        &self,
        offset: u32,
        document: Option<u32>,
    ) -> Result<Vec<SequencePoint>, Error> {
        let data = self.get_blob(offset)?;
        let (local_signature, mut data) = decode_unsigned(data)?;
        let mut current_document = match document {
            Some(document) => document,
            None => {
                let (initial_document, rest) = decode_unsigned(data)?;
                data = rest;
                initial_document
            }
        };

        let mut sequence_points = Vec::new();

        let (first_sequence_point, mut data) =
            SequencePoint::parse(data, None, None, current_document)?;

        sequence_points.push(first_sequence_point);

        let mut last_nonhidden =
            (!first_sequence_point.is_hidden()).then_some(first_sequence_point);

        while !data.is_empty() {
            if data[0] == 0 {
                let (doc, rest) = decode_unsigned(&data[1..])?;
                current_document = doc;
                data = rest;
                continue;
            }

            let (sequence_point, rest) = SequencePoint::parse(
                data,
                sequence_points.last().cloned(),
                last_nonhidden,
                current_document,
            )?;
            data = rest;

            sequence_points.push(sequence_point);
            if !sequence_point.is_hidden() {
                last_nonhidden = Some(sequence_point);
            }
        }

        Ok(sequence_points)
    }
}

#[derive(Debug, Clone)]
struct PdbStream<'data> {
    header: &'data raw::PdbStreamHeader,
    referenced_table_sizes: [u32; 64],
}

impl<'data> PdbStream<'data> {
    fn parse(buf: &'data [u8]) -> Result<Self, Error> {
        let (lv, mut rest) = LayoutVerified::<_, raw::PdbStreamHeader>::new_from_prefix(buf)
            .ok_or(ErrorKind::InvalidHeader)?;
        let header = lv.into_ref();

        let mut referenced_table_sizes = [0; 64];
        for (i, table) in referenced_table_sizes.iter_mut().enumerate() {
            if (header.referenced_tables >> i & 1) == 0 {
                continue;
            }

            let (lv, rest_) =
                LayoutVerified::<_, u32>::new_from_prefix(rest).ok_or(ErrorKind::InvalidLength)?;
            let len = lv.read();
            rest = rest_;

            *table = len as u32;
        }
        Ok(Self {
            header,
            referenced_table_sizes,
        })
    }
}

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Clone, Copy)]
struct UsStream<'data> {
    buf: &'data [u8],
}

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Clone, Copy)]
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

/// Decodes a compressed unsigned number at the start of a byte slice, returning the number
/// and the rest of the slice in the success case.
fn decode_unsigned(data: &[u8]) -> Result<(u32, &[u8]), Error> {
    let first_byte = *data.first().ok_or(ErrorKind::InvalidCompressedUnsigned)?;

    if first_byte & 0b1000_0000 == 0 {
        return Ok((first_byte as u32, &data[1..]));
    }

    if first_byte & 0b0100_0000 == 0 {
        let bytes = data.get(..2).ok_or(ErrorKind::InvalidCompressedUnsigned)?;
        let num = u16::from_be_bytes(bytes.try_into().unwrap());
        let masked = num & 0b0011_1111_1111_1111;
        return Ok((masked as u32, &data[2..]));
    }

    if first_byte & 0b0010_0000 == 0 {
        let bytes = data.get(..4).ok_or(ErrorKind::InvalidCompressedUnsigned)?;
        let num = u32::from_be_bytes(bytes.try_into().unwrap());
        let masked = num & 0b0001_1111_1111_1111_1111_1111_1111_1111;
        return Ok((masked, &data[4..]));
    }

    Err(ErrorKind::InvalidCompressedUnsigned.into())
}

/// Decodes a compressed signed number at the start of a byte slice, returning the number
/// and the rest of the slice in the success case.
fn decode_signed(data: &[u8]) -> Result<(i32, &[u8]), Error> {
    let first_byte = *data.first().ok_or(ErrorKind::InvalidCompressedSigned)?;

    if first_byte & 0b1000_0000 == 0 {
        // transform `0b0abc_defg` to `0bggab_cdef`.
        let lsb = first_byte & 0b0000_0001; // lsb = 0b0000_000g
        let mut rotated = first_byte >> 1; // rotated = 0b00ab_cdef
        rotated |= lsb << 6; // rotated = 0b0gab_cdef
        rotated |= lsb << 7; // rotated = 0bggab_cdef;
        return Ok((rotated as i8 as i32, &data[1..]));
    }

    if first_byte & 0b0100_0000 == 0 {
        let bytes = data.get(..2).ok_or(ErrorKind::InvalidCompressedSigned)?;
        let mut num = u16::from_be_bytes(bytes.try_into().unwrap());
        num &= 0b0011_1111_1111_1111; // clear the tag bits
        let lsb = num & 0b0000_0001;
        let mut rotated = num >> 1;
        rotated |= lsb << 13;
        rotated |= lsb << 14;
        rotated |= lsb << 15;
        return Ok((rotated as i16 as i32, &data[2..]));
    }

    if first_byte & 0b0010_0000 == 0 {
        let bytes = data.get(..4).ok_or(ErrorKind::InvalidCompressedSigned)?;
        let mut num = u32::from_be_bytes(bytes.try_into().unwrap());
        num &= 0b0001_1111_1111_1111_1111_1111_1111_1111; // clear the tag bits
        let lsb = num & 0b0000_0001;
        let mut rotated = num >> 1;
        rotated |= lsb << 28;
        rotated |= lsb << 29;
        rotated |= lsb << 30;
        rotated |= lsb << 31;
        return Ok((rotated as i32, &data[4..]));
    }

    Err(ErrorKind::InvalidCompressedSigned.into())
}

#[derive(Clone, Copy)]
struct SequencePoint {
    il_offset: u32,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    document_id: u32,
}

impl SequencePoint {
    /// Returns true if this is a "hidden" sequence point.
    fn is_hidden(&self) -> bool {
        self.start_line == 0xfeefee
            && self.end_line == 0xfeefee
            && self.start_column == 0
            && self.end_column == 0
    }

    fn new(
        il_offset: u32,
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
        document_id: u32,
    ) -> Result<Self, Error> {
        if il_offset >= 0x20000000
            || start_line >= 0x20000000
            || end_line >= 0x20000000
            || start_column >= 0x10000
            || end_column >= 0x10000
            || start_line == 0xfeefee
            || end_line == 0xfeefee
            || end_line < start_line
            || (end_line == start_line && end_column <= start_column)
        {
            Err(ErrorKind::InvalidSequencePoint.into())
        } else {
            Ok(Self {
                il_offset,
                start_line,
                start_column,
                end_line,
                end_column,
                document_id,
            })
        }
    }

    fn new_hidden(il_offset: u32, document_id: u32) -> Self {
        Self {
            il_offset,
            start_line: 0xfeefee,
            start_column: 0,
            end_line: 0xfeefee,
            end_column: 0,
            document_id,
        }
    }

    fn parse(
        data: &[u8],
        prev: Option<SequencePoint>,
        prev_non_hidden: Option<SequencePoint>,
        document_id: u32,
    ) -> Result<(Self, &[u8]), Error> {
        let (il_offset, data) = match prev {
            Some(prev) => {
                let (delta_il_offset, data) = decode_unsigned(data)?;
                (prev.il_offset + delta_il_offset, data)
            }
            None => decode_unsigned(data)?,
        };

        let (delta_lines, data) = decode_unsigned(data)?;
        let (delta_cols, data): (i32, &[u8]) = if delta_lines == 0 {
            let (n, data) = decode_unsigned(data)?;
            (n.try_into().unwrap(), data)
        } else {
            decode_signed(data)?
        };

        if delta_lines == 0 && delta_cols == 0 {
            return Ok((Self::new_hidden(il_offset, document_id), data));
        }

        let (start_line, data) = match prev_non_hidden {
            Some(prev) => {
                let (delta_start_line, data) = decode_unsigned(data)?;
                (prev.start_line + delta_start_line, data)
            }
            None => decode_unsigned(data)?,
        };

        let (start_column, data) = match prev_non_hidden {
            Some(prev) => {
                let (delta_start_col, data) = decode_unsigned(data)?;
                (prev.start_column + delta_start_col, data)
            }
            None => decode_unsigned(data)?,
        };

        let end_line = start_line + delta_lines;
        let end_column = (start_column as i32 + delta_cols) as u32;

        Ok((
            Self::new(
                il_offset,
                start_line,
                start_column,
                end_line,
                end_column,
                document_id,
            )?,
            data,
        ))
    }
}

impl fmt::Debug for SequencePoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_hidden() {
            f.debug_struct("HiddenSequencePoint")
                .field("il_offset", &self.il_offset)
                .field("document_id", &self.document_id)
                .finish()
        } else {
            f.debug_struct("SequencePoint")
                .field("il_offset", &self.il_offset)
                .field("start_line", &self.start_line)
                .field("start_column", &self.start_column)
                .field("end_line", &self.end_line)
                .field("end_column", &self.end_column)
                .field("document_id", &self.document_id)
                .finish()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::decode_signed;
    use super::decode_unsigned;

    #[test]
    fn test_decode_unsigned() {
        let cases = [
            (&[0x03][..], 0x03),
            (&[0x7F], 0x7F),
            (&[0x80, 0x80], 0x80),
            (&[0xAE, 0x57], 0x2E57),
            (&[0xBF, 0xFF], 0x3FFF),
            (&[0xC0, 0x00, 0x40, 0x00], 0x4000),
            (&[0xDF, 0xFF, 0xFF, 0xFF], 0x1FFF_FFFF),
        ];

        for (arg, res) in cases.iter() {
            assert_eq!(decode_unsigned(arg).unwrap().0, *res);
        }
    }
    #[test]
    fn test_decode_signed() {
        let cases = [
            (&[0x01][..], -64),
            (&[0x7E], 63),
            (&[0x7B], -3),
            (&[0x80, 0x80], 64),
            (&[0x80, 0x01], -8192),
            (&[0xC0, 0x00, 0x40, 0x00], 8192),
            (&[0xDF, 0xFF, 0xFF, 0xFE], 268435455),
            (&[0xC0, 0x00, 0x00, 0x01], -268435456),
        ];

        for (arg, res) in cases.iter() {
            assert_eq!(decode_signed(arg).unwrap().0, *res);
        }
    }
}

#[test]
fn test_ppdb() {
    let buf = std::fs::read("/Users/sebastian/code/unity/Runtime/Sentry.Unity.pdb").unwrap();

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

    dbg!(pdb.lookup(10, 17).unwrap());

    // let num_methods = table_stream[TableType::MethodDebugInformation].rows;
    // for method in 1..=num_methods {
    //     let doc = table_stream
    //         .get_u32(TableType::MethodDebugInformation, method, 1)
    //         .filter(|n| *n > 0);
    //     let blob_idx = table_stream
    //         .get_u32(TableType::MethodDebugInformation, method, 2)
    //         .filter(|n| *n > 0);

    //     if let Some(blob_idx) = blob_idx {
    //         dbg!(method);
    //         let sequence_points = pdb
    //             .get_sequence_points(blob_idx, doc)
    //             .unwrap()
    //             .sequence_points;

    //         dbg!(&sequence_points);

    //         for sp in sequence_points.iter() {
    //             let id = sp.document_id as usize;
    //             let doc_idx = table_stream.get_u32(TableType::Document, id, 1);
    //             match doc_idx {
    //                 Some(idx) => println!("{}", pdb.get_document_name(idx).unwrap()),
    //                 None => println!("no document"),
    //             }
    //         }
    //     }
    // }

    // dbg!(table_stream.get_row(TableType::LocalScope, 1));
}
