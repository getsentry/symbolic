mod blob;
mod metadata;
mod raw;
mod sequence_points;

use std::fmt;

use thiserror::Error;
use zerocopy::LayoutVerified;

use symbolic_common::Uuid;

use blob::BlobStream;
use metadata::{MetadataStream, TableType};

#[derive(Debug, Clone, Copy, Error)]
#[non_exhaustive]
pub enum FormatErrorKind {
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
}

#[derive(Debug, Error)]
#[error("{kind}")]
pub struct FormatError {
    pub(crate) kind: FormatErrorKind,
    #[source]
    pub(crate) source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl FormatError {
    /// Creates a new SymCache error from a known kind of error as well as an
    /// arbitrary error payload.
    pub(crate) fn new<E>(kind: FormatErrorKind, source: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let source = Some(source.into());
        Self { kind, source }
    }

    /// Returns the corresponding [`ErrorKind`] for this error.
    pub fn kind(&self) -> FormatErrorKind {
        self.kind
    }
}

impl From<FormatErrorKind> for FormatError {
    fn from(kind: FormatErrorKind) -> Self {
        Self { kind, source: None }
    }
}

#[derive(Clone)]
pub struct PortablePdb<'data> {
    /// First part of the metadata header.
    header: &'data raw::Header,
    /// The version string.
    version_string: &'data str,
    /// Second part of the metadata header.
    header2: &'data raw::HeaderPart2,
    /// The file's #PDB stream, if it exists.
    pdb_stream: Option<PdbStream<'data>>,
    /// The file's #~ stream, if it exists.
    table_stream: Option<MetadataStream<'data>>,
    /// The file's #Strings stream, if it exists.
    string_stream: Option<StringStream<'data>>,
    /// The file's #US stream, if it exists.
    us_stream: Option<UsStream<'data>>,
    /// The file's #Blob stream, if it exists.
    blob_stream: Option<BlobStream<'data>>,
    /// The file's #GUID stream, if it exists.
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
    /// Parses the provided buffer into a Portable PDB file.
    pub fn parse(buf: &'data [u8]) -> Result<Self, FormatError> {
        let (lv, rest) = LayoutVerified::<_, raw::Header>::new_from_prefix(buf)
            .ok_or(FormatErrorKind::InvalidHeader)?;
        let header = lv.into_ref();

        if header.signature != raw::METADATA_SIGNATURE {
            return Err(FormatErrorKind::InvalidSignature.into());
        }

        // TODO: verify major/minor version
        // TODO: verify reserved
        let version_length = header.version_length as usize;
        let version_buf = rest
            .get(..version_length)
            .ok_or(FormatErrorKind::InvalidLength)?;
        let version_buf = version_buf
            .split(|c| *c == 0)
            .next()
            .ok_or(FormatErrorKind::InvalidVersionString)?;
        let version = std::str::from_utf8(version_buf)
            .map_err(|e| FormatError::new(FormatErrorKind::InvalidVersionString, e))?;

        // We already know that buf is long enough.
        let streams_buf = &rest[version_length..];
        let (lv, mut streams_buf) =
            LayoutVerified::<_, raw::HeaderPart2>::new_from_prefix(streams_buf)
                .ok_or(FormatErrorKind::InvalidHeader)?;
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
                    .ok_or(FormatErrorKind::InvalidStreamHeader)?;
            let header = lv.into_ref();

            let name_buf = after_header_buf.get(..32).unwrap_or(after_header_buf);
            let name_buf = name_buf
                .split(|c| *c == 0)
                .next()
                .ok_or(FormatErrorKind::InvalidStreamName)?;
            let name = std::str::from_utf8(name_buf)
                .map_err(|e| FormatError::new(FormatErrorKind::InvalidStreamName, e))?;

            let mut rounded_name_len = name.len() + 1;
            rounded_name_len = match rounded_name_len % 4 {
                0 => rounded_name_len,
                r => rounded_name_len + (4 - r),
            };
            streams_buf = after_header_buf
                .get(rounded_name_len..)
                .ok_or(FormatErrorKind::InvalidLength)?;

            let offset = header.offset as usize;
            let size = header.size as usize;
            let stream_buf = buf
                .get(offset..offset + size)
                .ok_or(FormatErrorKind::InvalidLength)?;

            match name {
                "#Pdb" => result.pdb_stream = Some(PdbStream::parse(stream_buf)?),
                "#~" => {
                    result.table_stream = Some(MetadataStream::parse(
                        stream_buf,
                        result
                            .pdb_stream
                            .as_ref()
                            .map_or([0; 64], |s| s.referenced_table_sizes),
                    )?)
                }
                "#Strings" => result.string_stream = Some(StringStream { buf: stream_buf }),
                "#US" => result.us_stream = Some(UsStream { _buf: stream_buf }),
                "#Blob" => result.blob_stream = Some(BlobStream::new(stream_buf)),
                "#GUID" => result.guid_stream = Some(GuidStream::parse(stream_buf)?),
                _ => return Err(FormatErrorKind::UnknownStream.into()),
            }
        }
        Ok(result)
    }

    /// Reads the string starting at the given offset from this file's string heap.
    #[allow(unused)]
    fn get_string(&self, offset: u32) -> Result<&'data str, FormatError> {
        self.string_stream
            .as_ref()
            .ok_or(FormatErrorKind::NoStringsStream)?
            .get_string(offset)
    }

    /// Reads the GUID with the given index from this file's GUID heap.
    ///
    /// Note that the index is 1-based!
    fn get_guid(&self, idx: u32) -> Result<Uuid, FormatError> {
        self.guid_stream
            .as_ref()
            .ok_or(FormatErrorKind::NoGuidStream)?
            .get_guid(idx)
            .ok_or_else(|| FormatErrorKind::InvalidIndex.into())
    }

    /// Reads the blob starting at the given offset from this file's blob heap.
    fn get_blob(&self, offset: u32) -> Result<&'data [u8], FormatError> {
        self.blob_stream
            .as_ref()
            .ok_or(FormatErrorKind::NoBlobStream)?
            .get_blob(offset)
    }

    /// Reads this file's PDB ID from its #PDB stream.
    pub(crate) fn pdb_id(&self) -> Option<[u8; 20]> {
        self.pdb_stream.as_ref().map(|stream| stream.id())
    }

    /// Reads the `(row, col)` cell in the given table as a `u32`.
    ///
    /// This returns an error if the indices are out of bounds for the table
    /// or the cell is too wide for a `u32`.
    ///
    /// Note that row and column indices are 1-based!
    pub(crate) fn get_table_cell_u32(
        &self,
        table: TableType,
        row: usize,
        col: usize,
    ) -> Result<u32, FormatError> {
        let md_stream = self
            .table_stream
            .as_ref()
            .ok_or(FormatErrorKind::NoMetadataStream)?;
        md_stream.get_table_cell_u32(table, row, col)
    }
}

/// The file's #PDB stream.
///
/// See https://github.com/dotnet/runtime/blob/main/docs/design/specs/PortablePdb-Metadata.md#pdb-stream.
#[derive(Debug, Clone)]
struct PdbStream<'data> {
    header: &'data raw::PdbStreamHeader,
    referenced_table_sizes: [u32; 64],
}

impl<'data> PdbStream<'data> {
    fn parse(buf: &'data [u8]) -> Result<Self, FormatError> {
        let (lv, mut rest) = LayoutVerified::<_, raw::PdbStreamHeader>::new_from_prefix(buf)
            .ok_or(FormatErrorKind::InvalidHeader)?;
        let header = lv.into_ref();

        let mut referenced_table_sizes = [0; 64];
        for (i, table) in referenced_table_sizes.iter_mut().enumerate() {
            if (header.referenced_tables >> i & 1) == 0 {
                continue;
            }

            let (lv, rest_) = LayoutVerified::<_, u32>::new_from_prefix(rest)
                .ok_or(FormatErrorKind::InvalidLength)?;
            let len = lv.read();
            rest = rest_;

            *table = len as u32;
        }
        Ok(Self {
            header,
            referenced_table_sizes,
        })
    }

    fn id(&self) -> [u8; 20] {
        self.header.id
    }
}

/// A stream representing the "string heap", which contains UTF-8 string data.
///
/// See https://github.com/stakx/ecma-335/blob/master/docs/ii.24.2.3-strings-heap.md.
#[derive(Debug, Clone, Copy)]
struct StringStream<'data> {
    buf: &'data [u8],
}

impl<'data> StringStream<'data> {
    fn get_string(&self, offset: u32) -> Result<&'data str, FormatError> {
        let string_buf = self
            .buf
            .get(offset as usize..)
            .ok_or(FormatErrorKind::InvalidStringOffset)?;
        let string = string_buf.split(|c| *c == 0).next().unwrap();
        std::str::from_utf8(string)
            .map_err(|e| FormatError::new(FormatErrorKind::InvalidStringData, e))
    }
}

/// A stream representing the "user string heap".
///
/// See https://github.com/stakx/ecma-335/blob/master/docs/ii.24.2.4-us-and-blob-heaps.md.
#[derive(Debug, Clone, Copy)]
struct UsStream<'data> {
    _buf: &'data [u8],
}

/// A stream representing the "GUID heap", which contains GUIDs.
///
/// See https://github.com/stakx/ecma-335/blob/master/docs/ii.24.2.5-guid-heap.md.
#[derive(Debug, Clone, Copy)]
struct GuidStream<'data> {
    buf: &'data [uuid::Bytes],
}

impl<'data> GuidStream<'data> {
    fn parse(buf: &'data [u8]) -> Result<Self, FormatError> {
        let bytes = LayoutVerified::<_, [uuid::Bytes]>::new_slice(buf)
            .ok_or(FormatErrorKind::InvalidLength)?;

        Ok(Self {
            buf: bytes.into_slice(),
        })
    }

    fn get_guid(&self, idx: u32) -> Option<Uuid> {
        self.buf
            .get(idx.checked_sub(1)? as usize)
            .map(|bytes| Uuid::from_bytes_le(*bytes))
    }
}
