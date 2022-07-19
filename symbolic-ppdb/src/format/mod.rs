mod metadata;
mod raw;
mod sequence_points;
mod streams;
mod utils;

use std::fmt;

use thiserror::Error;
use zerocopy::LayoutVerified;

use symbolic_common::Uuid;

use metadata::{MetadataStream, TableType};
use streams::{BlobStream, GuidStream, PdbStream, StringStream, UsStream};

/// The kind of a [`FormatError`].
#[derive(Debug, Clone, Copy, Error)]
#[non_exhaustive]
pub enum FormatErrorKind {
    /// The header of the Portable PDB file could not be read.
    #[error("invalid header")]
    InvalidHeader,
    #[error("invalid signature")]
    /// The header of the Portable PDB does not contain the correct signature.
    InvalidSignature,
    /// The file ends prematurely.
    #[error("invalid length")]
    InvalidLength,
    /// The file does not contain a valid version string.
    #[error("invalid version string")]
    InvalidVersionString,
    /// A stream header could not be read.
    #[error("invalid stream header")]
    InvalidStreamHeader,
    /// A stream's name could not be read.
    #[error("invalid stream name")]
    InvalidStreamName,
    /// String data was requested, but the file does not contain a `#Strings` stream.
    #[error("file does not contain a #Strings stream")]
    NoStringsStream,
    /// The given offset is out of bounds for the string heap.
    #[error("invalid string offset")]
    InvalidStringOffset,
    /// Tried to read invalid string data.
    #[error("invalid string data")]
    InvalidStringData,
    /// An unrecognized stream name was encountered.
    #[error("unknown stream")]
    UnknownStream,
    /// GUID data was requested, but the file does not contain a `#GUID` stream.
    #[error("file does not contain a #Guid stream")]
    NoGuidStream,
    /// The given index is out of bounds for the GUID heap.
    #[error("invalid guid index")]
    InvalidGuidIndex,
    /// The table stream is too small to hold all claimed tables.
    #[error(
        "insufficient table data: {0} bytes required, but table stream only contains {1} bytes"
    )]
    InsufficientTableData(usize, usize),
    /// The given offset is out of bounds for the `#Blob` heap.
    #[error("invalid blob offset")]
    InvalidBlobOffset,
    /// The given offset points to invalid blob data.
    #[error("invalid blob data")]
    InvalidBlobData,
    /// Blob data was requested, but the file does not contain a `#Blob` stream.
    #[error("file does not contain a #Blob stream")]
    NoBlobStream,
    /// Tried to read an invalid compressed unsigned number.
    #[error("invalid compressed unsigned number")]
    InvalidCompressedUnsigned,
    /// Tried to read an invalid compressed signed number.
    #[error("invalid compressed signed number")]
    InvalidCompressedSigned,
    /// Could not read a document name.
    #[error("invalid document name")]
    InvalidDocumentName,
    /// Failed to parse a sequence point.
    #[error("invalid sequence point")]
    InvalidSequencePoint,
    /// Table data was requested, but the file does not contain a `#~` stream.
    #[error("file does not contain a #~ stream")]
    NoMetadataStream,
    /// The given row index is out of bounds for the table.
    #[error("row index {1} is out of bounds for table {0:?}")]
    RowIndexOutOfBounds(TableType, usize),
    /// The given column index is out of bounds for the table.
    #[error("column index {1} is out of bounds for table {0:?}")]
    ColIndexOutOfBounds(TableType, usize),
    /// The given column in the table has an incompatible width.
    #[error("column {1} in table {0:?} has incompatible width {2}")]
    ColumnWidth(TableType, usize, usize),
}

/// An error encountered while parsing a [`PortablePdb`] file.
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

    /// Returns the corresponding [`FormatErrorKind`] for this error.
    pub fn kind(&self) -> FormatErrorKind {
        self.kind
    }
}

impl From<FormatErrorKind> for FormatError {
    fn from(kind: FormatErrorKind) -> Self {
        Self { kind, source: None }
    }
}

/// A parsed Portable PDB file.
///
/// This can be converted to a [`PortablePdbCache`](crate::PortablePdbCache) using the
/// [`PortablePdbCacheConverter::process_portable_pdb`](crate::PortablePdbCacheConverter::process_portable_pdb)
/// method.
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
    metadata_stream: Option<MetadataStream<'data>>,
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
            .field("has_table_stream", &self.metadata_stream.is_some())
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
            metadata_stream: None,
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
                    result.metadata_stream = Some(MetadataStream::parse(
                        stream_buf,
                        result
                            .pdb_stream
                            .as_ref()
                            .map_or([0; 64], |s| s.referenced_table_sizes),
                    )?)
                }
                "#Strings" => result.string_stream = Some(StringStream::new(stream_buf)),
                "#US" => result.us_stream = Some(UsStream::new(stream_buf)),
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
            .ok_or_else(|| FormatErrorKind::InvalidGuidIndex.into())
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
            .metadata_stream
            .as_ref()
            .ok_or(FormatErrorKind::NoMetadataStream)?;
        md_stream.get_table_cell_u32(table, row, col)
    }
}
