use std::{
    convert::TryInto,
    fmt,
    ops::{Index, IndexMut},
    ptr,
};

use symbolic_common::Uuid;
use thiserror::Error;
use zerocopy::LayoutVerified;

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
                "#GUID" => {
                    result.guid_stream = {
                        dbg!(offset, size);
                        Some(GuidStream::parse(stream_buf)?)
                    }
                }
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

#[derive(Debug, Clone, Copy)]
enum IndexSize {
    U16,
    U32,
}

impl IndexSize {
    fn num_bytes(self) -> usize {
        match self {
            Self::U16 => 2,
            Self::U32 => 4,
        }
    }

    fn read(self, buf: &[u8]) -> Option<(u32, &[u8])> {
        if buf.len() < self.num_bytes() {
            return None;
        }

        let (first, rest) = buf.split_at(self.num_bytes());
        let num = match self {
            IndexSize::U16 => u16::from_ne_bytes(first.try_into().unwrap()) as u32,
            IndexSize::U32 => u32::from_ne_bytes(first.try_into().unwrap()),
        };

        Some((num, rest))
    }
}

#[derive(Debug, Clone, Copy)]
struct IndexSizes {
    string: IndexSize,
    guid: IndexSize,
    blob: IndexSize,
}

#[derive(Debug)]
struct TableStream<'data> {
    header: &'data raw::TableStreamHeader,
    tables: [Table; 64],
    table_contents: &'data [u8],
}

impl<'data> TableStream<'data> {
    pub fn parse(buf: &'data [u8]) -> Result<Self, Error> {
        dbg!(buf);
        let (lv, mut rest) = LayoutVerified::<_, raw::TableStreamHeader>::new_from_prefix(buf)
            .ok_or(ErrorKind::InvalidHeader)?;
        let header = lv.into_ref();

        // TODO: verify major/minor version
        // TODO: verify reserved

        let mut tables = [Table::default(); 64];
        for i in 0..64 {
            if (header.valid_tables >> i & 1) == 0 {
                continue;
            }

            let (lv, rest_) =
                LayoutVerified::<_, u32>::new_from_prefix(rest).ok_or(ErrorKind::InvalidLength)?;
            let len = lv.read();
            rest = rest_;

            tables[i].len = len as usize;
        }

        let table_contents = rest;
        Ok(Self {
            header,
            table_contents,
            tables,
        })
    }

    fn string_index_size(&self) -> IndexSize {
        if self.header.heap_sizes & 0x1 == 0 {
            IndexSize::U16
        } else {
            IndexSize::U32
        }
    }

    fn guid_index_size(&self) -> IndexSize {
        if self.header.heap_sizes & 0x2 == 0 {
            IndexSize::U16
        } else {
            IndexSize::U32
        }
    }

    fn blob_index_size(&self) -> IndexSize {
        if self.header.heap_sizes & 0x4 == 0 {
            IndexSize::U16
        } else {
            IndexSize::U32
        }
    }

    fn index_sizes(&self) -> IndexSizes {
        IndexSizes {
            string: self.string_index_size(),
            guid: self.guid_index_size(),
            blob: self.blob_index_size(),
        }
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

#[derive(Debug, Clone, Copy)]
struct ModuleRow {
    name_idx: u32,
    mvid_idx: u32,
}

impl ModuleRow {
    fn parse(buf: &[u8], index_sizes: IndexSizes) -> Result<Self, Error> {
        dbg!(&buf[..Self::size(index_sizes)]);
        if buf.len() < Self::size(index_sizes) {
            return Err(ErrorKind::InvalidLength.into());
        }

        let (name_idx, rest) = index_sizes.string.read(&buf[2..]).unwrap();
        let (mvid_idx, _) = index_sizes.guid.read(rest).unwrap();

        Ok(Self { name_idx, mvid_idx })
    }

    fn size(index_sizes: IndexSizes) -> usize {
        let generation = 2;
        let name = index_sizes.string.num_bytes();
        let mvid = index_sizes.guid.num_bytes();
        let enc_id = index_sizes.guid.num_bytes();
        let enc_base_id = index_sizes.guid.num_bytes();

        generation + name + mvid + enc_id + enc_base_id
    }
}

#[repr(usize)]
#[derive(Debug, Clone, Copy)]
enum TableType {
    Assembly = 0x20,
    AssemblyOs = 0x22,
    AssemblyProcessor = 0x21,
    AssemblyRef = 0x23,
    AssemblyRefOs = 0x25,
    AssemblyRefProcessor = 0x24,
    ClassLayout = 0x0F,
    Constant = 0x0B,
    CustomAttribute = 0x0C,
    DeclSecurity = 0x0E,
    EventMap = 0x12,
    Event = 0x14,
    ExportedType = 0x27,
    Field = 0x04,
    FieldLayout = 0x10,
    FieldMarshal = 0x0D,
    FieldRVA = 0x1D,
    File = 0x26,
    GenericParam = 0x2A,
    GenericParamConstraint = 0x2C,
    ImplMap = 0x1C,
    InterfaceImpl = 0x09,
    ManifestResource = 0x28,
    MemberRef = 0x0A,
    MethodDef = 0x06,
    MethodImpl = 0x19,
    MethodSemantics = 0x18,
    MethodSpec = 0x2B,
    Module = 0x00,
    ModuleRef = 0x1A,
    NestedClass = 0x29,
    Param = 0x08,
    Property = 0x17,
    PropertyMap = 0x15,
    StandAloneSig = 0x11,
    TypeDef = 0x02,
    TypeRef = 0x01,
    TypeSpec = 0x1B,
    // portable pdb extension starts here
    CustomDebugInformation = 0x37,
    Document = 0x30,
    ImportScope = 0x35,
    LocalConstant = 0x34,
    LocalScope = 0x32,
    LocalVariable = 0x33,
    MethodDebugInformation = 0x31,
    StateMachineMethod = 0x36,
}

#[derive(Debug, Default, Clone, Copy)]
struct Table {
    offset: usize,
    len: usize,
    width: usize,
    columns: [Column; 6],
}

impl Table {
    fn set_columns(
        &mut self,
        width0: usize,
        width1: usize,
        width2: usize,
        width3: usize,
        width4: usize,
        width5: usize,
    ) {
        self.columns[0].offset = 0;
        self.columns[0].width = width0;

        if width1 != 0 {
            self.columns[1].offset = self.columns[0].end();
            self.columns[1].width = width1;
        }

        if width2 != 0 {
            self.columns[2].offset = self.columns[1].end();
            self.columns[2].width = width2;
        }

        if width3 != 0 {
            self.columns[3].offset = self.columns[2].end();
            self.columns[3].width = width3;
        }

        if width4 != 0 {
            self.columns[4].offset = self.columns[3].end();
            self.columns[4].width = width4;
        }

        if width5 != 0 {
            self.columns[5].offset = self.columns[4].end();
            self.columns[5].width = width5;
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct Column {
    offset: usize,
    width: usize,
}

impl Column {
    fn end(self) -> usize {
        self.offset + self.width
    }
}

#[derive(Debug)]
struct Tables([Table; 64]);

impl Tables {
    fn set_columns(&mut self, index_sizes: IndexSizes) {
        use TableType::*;

        let assembly_ref_index_size = self.index_size(AssemblyRef);
        let type_def_index_size = self.index_size(TypeDef);

        let has_constant = self.composite_index_size(&[Field, Param, Property]);

        let (string_index_size, blob_index_size, guid_index_size) = (
            index_sizes.string.num_bytes(),
            index_sizes.blob.num_bytes(),
            index_sizes.guid.num_bytes(),
        );

        self[Assembly].set_columns(
            4,
            8,
            4,
            blob_index_size,
            string_index_size,
            string_index_size,
        );
        self[AssemblyOs].set_columns(4, 4, 4, 0, 0, 0);
        self[AssemblyProcessor].set_columns(4, 0, 0, 0, 0, 0);
        self[AssemblyRef].set_columns(
            8,
            4,
            blob_index_size,
            string_index_size,
            string_index_size,
            blob_index_size,
        );
        self[AssemblyRefOs].set_columns(4, 4, 4, assembly_ref_index_size, 0, 0);
        self[AssemblyRefProcessor].set_columns(4, assembly_ref_index_size, 0, 0, 0, 0);
        self[ClassLayout].set_columns(2, 4, type_def_index_size, 0, 0, 0);
        self[Constant].set_columns(2, has_constant, blob_index_size, 0, 0, 0);
    }

    fn index_size(&self, table: TableType) -> usize {
        if self[table].len >= u16::MAX as usize {
            4
        } else {
            2
        }
    }

    /// Computes the size (2 or 4 bytes) for an index into any of the tables in `tables`.
    ///
    /// This depends on the number of tables (because some part of the index needs to be used
    /// as a tag) and their maximum size.
    fn composite_index_size(&self, tables: &[TableType]) -> usize {
        /// Checks if `row_count` is less than 2^(16 - bits).
        fn is_small(row_count: usize, bits: u8) -> bool {
            (row_count as u64) < (1u64 << (16 - bits))
        }

        /// Calculates ceil(log₂(num_tables)) by repeated bit shifting.
        fn tag_bits(num_tables: usize) -> u8 {
            let mut num_tables = num_tables - 1;
            let mut bits: u8 = 1;
            loop {
                num_tables >>= 1;
                if num_tables == 0 {
                    break;
                }
                bits += 1;
            }
            bits
        }

        let bits_needed = tag_bits(tables.len());
        if tables
            .iter()
            .map(|table| self[*table].len)
            .all(|row_count| is_small(row_count, bits_needed))
        {
            2
        } else {
            4
        }
    }
}

impl Default for Tables {
    fn default() -> Self {
        Self([Table::default(); 64])
    }
}

impl Index<TableType> for Tables {
    type Output = Table;

    fn index(&self, index: TableType) -> &Self::Output {
        &self.0[index as usize]
    }
}

impl IndexMut<TableType> for Tables {
    fn index_mut(&mut self, index: TableType) -> &mut Self::Output {
        &mut self.0[index as usize]
    }
}

#[test]
fn test_ppdb() {
    let buf = std::fs::read("_fixtures/Documents.pdbx").unwrap();

    let pdb = PortablePdb::parse(&buf).unwrap();
    println!("{pdb:#?}");

    let table_stream = pdb.table_stream.as_ref().unwrap();
    dbg!(table_stream.header);
    for (i, table) in table_stream
        .tables
        .iter()
        .enumerate()
        .filter(|(_, table)| table.len > 0)
    {
        println!("{i:#0x}: {table:?}");
    }
}
