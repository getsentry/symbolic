mod metadata;
mod raw;
mod sequence_points;
mod sourcelinks;
mod streams;
mod utils;

use std::{borrow::Cow, fmt, io::Read};

use flate2::read::DeflateDecoder;
use thiserror::Error;
use watto::Pod;

use symbolic_common::{DebugId, Language, Uuid};

use metadata::{
    CustomDebugInformation, CustomDebugInformationIterator, MetadataStream, Table, TableType,
};
use sourcelinks::SourceLinkMappings;
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
    /// Tried to read an custom debug information table item tag.
    #[error("invalid custom debug information table item tag {0}")]
    InvalidCustomDebugInformationTag(u32),
    /// Tried to read contents of a blob in an unknown format.
    #[error("invalid blob format {0}")]
    InvalidBlobFormat(u32),
    /// Failed to parse Source Link JSON
    #[error("invalid source link JSON")]
    InvalidSourceLinkJson,
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
    /// Creates a new FormatError error from a known kind of error as well as an
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
    /// Source link mappings
    source_link_mappings: SourceLinkMappings,
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
    /// Checks whether the provided buffer could potentially be a Portable PDB file,
    /// without fully parsing it.
    pub fn peek(buf: &[u8]) -> bool {
        if let Some((header, _)) = raw::Header::ref_from_prefix(buf) {
            return header.signature == raw::METADATA_SIGNATURE;
        }
        false
    }

    /// Parses the provided buffer into a Portable PDB file.
    pub fn parse(buf: &'data [u8]) -> Result<Self, FormatError> {
        let (header, rest) =
            raw::Header::ref_from_prefix(buf).ok_or(FormatErrorKind::InvalidHeader)?;

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
        let (header2, mut streams_buf) =
            raw::HeaderPart2::ref_from_prefix(streams_buf).ok_or(FormatErrorKind::InvalidHeader)?;

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
            source_link_mappings: SourceLinkMappings::empty(),
        };

        let mut metadata_stream = None;
        for _ in 0..stream_count {
            let (header, after_header_buf) = raw::StreamHeader::ref_from_prefix(streams_buf)
                .ok_or(FormatErrorKind::InvalidStreamHeader)?;

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
                // Save the #~ stream for last; it definitely must be parsed after the #Pdb stream.
                "#~" => metadata_stream = Some(stream_buf),
                "#Strings" => result.string_stream = Some(StringStream::new(stream_buf)),
                "#US" => result.us_stream = Some(UsStream::new(stream_buf)),
                "#Blob" => result.blob_stream = Some(BlobStream::new(stream_buf)),
                "#GUID" => result.guid_stream = Some(GuidStream::parse(stream_buf)?),
                _ => return Err(FormatErrorKind::UnknownStream.into()),
            }
        }

        if let Some(stream_buf) = metadata_stream {
            result.metadata_stream = Some(MetadataStream::parse(
                stream_buf,
                result
                    .pdb_stream
                    .as_ref()
                    .map_or([0; 64], |s| s.referenced_table_sizes),
            )?)
        }

        // Read source link mappings.
        // https://github.com/dotnet/runtime/blob/main/docs/design/specs/PortablePdb-Metadata.md#source-link-c-and-vb-compilers
        const SOURCE_LINK_KIND: Uuid = uuid::uuid!("CC110556-A091-4D38-9FEC-25AB9A351A6A");
        let mut source_link_mappings = Vec::new();
        for cdi in CustomDebugInformationIterator::new(&result, SOURCE_LINK_KIND)? {
            let cdi = cdi?;
            // Note: only handle module #1 (do we actually handle multiple modules in any way??)
            if let (metadata::CustomDebugInformationTag::Module, 1) = (cdi.tag, cdi.value) {
                let json = String::from_utf8_lossy(result.get_blob(cdi.blob)?);
                source_link_mappings.push(json);
            }
        }
        result.source_link_mappings =
            SourceLinkMappings::new(source_link_mappings.iter().map(|v| v.as_ref()))?;

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
    pub fn pdb_id(&self) -> Option<DebugId> {
        self.pdb_stream.as_ref().map(|stream| stream.id())
    }

    /// Reads the `(row, col)` cell in the given table as a `u32`.
    ///
    /// This returns an error if the indices are out of bounds for the table
    /// or the cell is too wide for a `u32`.
    ///
    /// Note that row and column indices are 1-based!
    pub(crate) fn get_table(&self, table: TableType) -> Result<Table, FormatError> {
        let md_stream = self
            .metadata_stream
            .as_ref()
            .ok_or(FormatErrorKind::NoMetadataStream)?;
        Ok(md_stream[table])
    }

    /// Returns true if this portable pdb file contains method debug information.
    pub fn has_debug_info(&self) -> bool {
        self.metadata_stream.as_ref().map_or(false, |md_stream| {
            md_stream[TableType::MethodDebugInformation].rows > 0
        })
    }

    /// Get source file referenced by this PDB.
    ///
    /// Given index must be between 1 and get_documents_count().
    pub fn get_document(&self, idx: usize) -> Result<Document, FormatError> {
        let table = self.get_table(TableType::Document)?;
        let row = table.get_row(idx)?;
        let name_offset = row.get_col_u32(1)?;
        let lang_offset = row.get_col_u32(4)?;

        let name = self.get_document_name(name_offset)?;
        let lang = self.get_document_lang(lang_offset)?;

        Ok(Document { name, lang })
    }

    /// Get the number of source files referenced by this PDB.
    pub fn get_documents_count(&self) -> Result<usize, FormatError> {
        let table = self.get_table(TableType::Document)?;
        Ok(table.rows)
    }

    /// An iterator over source files contents' embedded in this PDB.
    pub fn get_embedded_sources(&self) -> Result<EmbeddedSourceIterator<'_, 'data>, FormatError> {
        EmbeddedSourceIterator::new(self)
    }

    /// Tries to resolve given document as a source link (URL).
    /// Make sure to try [Self::get_embedded_sources] first when looking for a source file, because
    /// function may return a link that actually doesn't exist (e.g. file is in .gitignore).
    /// In that case, it's usually the case that the file is embedded in the PPDB instead.
    pub fn get_source_link(&self, document: &Document) -> Option<Cow<'_, str>> {
        self.source_link_mappings
            .resolve(&document.name)
            .map(Cow::Owned)
    }
}

/// Represents a source file that is referenced by this PDB.
#[derive(Debug, Clone)]
pub struct Document {
    /// Document names are usually normalized full paths.
    pub name: String,
    pub(crate) lang: Language,
}

/// An iterator over Embedded Sources.
#[derive(Debug, Clone)]
pub struct EmbeddedSourceIterator<'object, 'data> {
    ppdb: &'object PortablePdb<'data>,
    inner_it: CustomDebugInformationIterator<'data>,
}

impl<'object, 'data> EmbeddedSourceIterator<'object, 'data> {
    fn new(ppdb: &'object PortablePdb<'data>) -> Result<Self, FormatError> {
        // https://github.com/dotnet/runtime/blob/main/docs/design/specs/PortablePdb-Metadata.md#embedded-source-c-and-vb-compilers
        const EMBEDDED_SOURCES_KIND: Uuid = uuid::uuid!("0E8A571B-6926-466E-B4AD-8AB04611F5FE");
        let inner_it = CustomDebugInformationIterator::new(ppdb, EMBEDDED_SOURCES_KIND)?;
        Ok(EmbeddedSourceIterator { ppdb, inner_it })
    }

    fn get_source(
        &mut self,
        info: CustomDebugInformation,
    ) -> Result<EmbeddedSource<'data>, FormatError> {
        let document = self.ppdb.get_document(info.value as usize)?;
        let blob = self.ppdb.get_blob(info.blob)?;
        Ok(EmbeddedSource { document, blob })
    }
}

impl<'object, 'data> Iterator for EmbeddedSourceIterator<'object, 'data> {
    type Item = Result<EmbeddedSource<'data>, FormatError>;

    fn next(&mut self) -> Option<Self::Item> {
        // Skip rows that are not "Document". From the specs, it should always be the case but we've
        // had a MethodDef (with an invalid 0 row index...) in the field so there's a test for it.
        while let Some(row) = self.inner_it.next() {
            match row {
                Err(e) => return Some(Err(e)),
                Ok(info) => {
                    if let metadata::CustomDebugInformationTag::Document = info.tag {
                        return Some(self.get_source(info));
                    }
                }
            }
        }
        None
    }
}

/// Lazy Embedded Source file reader.
#[derive(Debug, Clone)]
pub struct EmbeddedSource<'data> {
    document: Document,
    blob: &'data [u8],
}

impl<'data, 'object> EmbeddedSource<'data> {
    /// Returns the build-time path associated with this source file.
    pub fn get_path(&'object self) -> &'object str {
        self.document.name.as_str()
    }

    /// Reads the source file contents from the Portable PDB.
    pub fn get_contents(&self) -> Result<Cow<'data, [u8]>, FormatError> {
        // The blob has the following structure: `Blob ::= format content`
        // - format - int32 - Indicates how the content is serialized.
        //     0 = raw bytes, uncompressed.
        //     Positive value = compressed by deflate algorithm and value indicates uncompressed size.
        //     Negative values reserved for future formats.
        // - content - format-specific - The text of the document in the specified format. The length is implied by the length of the blob minus four bytes for the format.
        if self.blob.len() < 4 {
            return Err(FormatErrorKind::InvalidBlobData.into());
        }
        let (format_blob, data_blob) = self.blob.split_at(4);
        let format = u32::from_ne_bytes(format_blob.try_into().unwrap());
        match format {
            0 => Ok(Cow::Borrowed(data_blob)),
            x if x > 0 => self.inflate_contents(format as usize, data_blob),
            _ => Err(FormatErrorKind::InvalidBlobFormat(format).into()),
        }
    }

    fn inflate_contents(
        &self,
        size: usize,
        data: &'data [u8],
    ) -> Result<Cow<'data, [u8]>, FormatError> {
        let mut decoder = DeflateDecoder::new(data);
        let mut output = Vec::with_capacity(size);
        let read_size = decoder
            .read_to_end(&mut output)
            .map_err(|e| FormatError::new(FormatErrorKind::InvalidBlobData, e))?;
        if read_size != size {
            return Err(FormatErrorKind::InvalidLength.into());
        }
        Ok(Cow::Owned(output))
    }
}
