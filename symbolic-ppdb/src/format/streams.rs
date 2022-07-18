use symbolic_common::Uuid;
use zerocopy::LayoutVerified;

use super::raw::PdbStreamHeader;
use super::utils::decode_unsigned;
use super::{FormatError, FormatErrorKind};

/// A stream representing the "blob heap", which contains "blobs" of arbitrary binary data.
///
/// See https://github.com/stakx/ecma-335/blob/master/docs/ii.24.2.4-us-and-blob-heaps.md.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BlobStream<'data> {
    buf: &'data [u8],
}

impl<'data> BlobStream<'data> {
    pub(crate) fn new(buf: &'data [u8]) -> Self {
        Self { buf }
    }

    /// Gets the blob starting at the specified offset out of the blob heap.
    pub(crate) fn get_blob(&self, offset: u32) -> Result<&'data [u8], FormatError> {
        let offset = offset as usize;
        let (len, rest) = decode_unsigned(
            self.buf
                .get(offset..)
                .ok_or(FormatErrorKind::InvalidBlobOffset)?,
        )?;

        rest.get(..len as usize)
            .ok_or_else(|| FormatErrorKind::InvalidBlobData.into())
    }
}

/// The file's #PDB stream.
///
/// See https://github.com/dotnet/runtime/blob/main/docs/design/specs/PortablePdb-Metadata.md#pdb-stream.
#[derive(Debug, Clone)]
pub(crate) struct PdbStream<'data> {
    header: &'data PdbStreamHeader,
    pub(crate) referenced_table_sizes: [u32; 64],
}

impl<'data> PdbStream<'data> {
    pub(crate) fn parse(buf: &'data [u8]) -> Result<Self, FormatError> {
        let (lv, mut rest) = LayoutVerified::<_, PdbStreamHeader>::new_from_prefix(buf)
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

    pub(crate) fn id(&self) -> [u8; 20] {
        self.header.id
    }
}

/// A stream representing the "string heap", which contains UTF-8 string data.
///
/// See https://github.com/stakx/ecma-335/blob/master/docs/ii.24.2.3-strings-heap.md.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StringStream<'data> {
    buf: &'data [u8],
}

impl<'data> StringStream<'data> {
    pub(crate) fn new(buf: &'data [u8]) -> Self {
        Self { buf }
    }

    pub(crate) fn get_string(&self, offset: u32) -> Result<&'data str, FormatError> {
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
pub(crate) struct UsStream<'data> {
    _buf: &'data [u8],
}
impl<'data> UsStream<'data> {
    pub(crate) fn new(buf: &'data [u8]) -> Self {
        Self { _buf: buf }
    }
}
/// A stream representing the "GUID heap", which contains GUIDs.
///
/// See https://github.com/stakx/ecma-335/blob/master/docs/ii.24.2.5-guid-heap.md.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GuidStream<'data> {
    buf: &'data [uuid::Bytes],
}

impl<'data> GuidStream<'data> {
    pub(crate) fn parse(buf: &'data [u8]) -> Result<Self, FormatError> {
        let bytes = LayoutVerified::<_, [uuid::Bytes]>::new_slice(buf)
            .ok_or(FormatErrorKind::InvalidLength)?;

        Ok(Self {
            buf: bytes.into_slice(),
        })
    }

    pub(crate) fn get_guid(&self, idx: u32) -> Option<Uuid> {
        self.buf
            .get(idx.checked_sub(1)? as usize)
            .map(|bytes| Uuid::from_bytes_le(*bytes))
    }
}
