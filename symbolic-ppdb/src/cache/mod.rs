mod lookup;
pub(crate) mod raw;
pub(crate) mod writer;

use thiserror::Error;
use zerocopy::LayoutVerified;

const PPDBCACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Error)]
pub enum CacheErrorKind {
    #[error("could not read header")]
    InvalidHeader,
    #[error("wrong endianness")]
    WrongEndianness,
    #[error("invalid magic: {0}")]
    InvalidMagic(u32),
    #[error("wrong version: {0}")]
    WrongVersion(u32),
    #[error("could not read ranges")]
    InvalidRanges,
    #[error("could not read source locations")]
    InvalidSourceLocations,
    #[error("expected {expected} string bytes, found {found}")]
    UnexpectedStringBytes { expected: usize, found: usize },
    #[error("error processing portable pdb file")]
    PortablePdb,
}

#[derive(Debug, Error)]
#[error("{kind}")]
pub struct CacheError {
    pub(crate) kind: CacheErrorKind,
    #[source]
    pub(crate) source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl CacheError {
    /// Creates a new SymCache error from a known kind of error as well as an
    /// arbitrary error payload.
    pub(crate) fn new<E>(kind: CacheErrorKind, source: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let source = Some(source.into());
        Self { kind, source }
    }

    /// Returns the corresponding [`ErrorKind`] for this error.
    pub fn kind(&self) -> CacheErrorKind {
        self.kind
    }
}

impl From<CacheErrorKind> for CacheError {
    fn from(kind: CacheErrorKind) -> Self {
        Self { kind, source: None }
    }
}

impl From<crate::Error> for CacheError {
    fn from(e: crate::Error) -> Self {
        Self::new(CacheErrorKind::PortablePdb, e)
    }
}

#[derive(Debug)]
pub struct PortablePdbCache<'data> {
    header: &'data raw::Header,
    pub(crate) source_locations: &'data [raw::SourceLocation],
    pub(crate) ranges: &'data [raw::Range],
    string_bytes: &'data [u8],
}

impl<'data> PortablePdbCache<'data> {
    pub fn parse(buf: &'data [u8]) -> Result<Self, CacheError> {
        let (lv, rest) = LayoutVerified::<_, raw::Header>::new_from_prefix(buf)
            .ok_or(CacheErrorKind::InvalidHeader)?;

        let header = lv.into_ref();

        if header.magic == raw::PPDBCACHE_MAGIC_FLIPPED {
            return Err(CacheErrorKind::WrongEndianness.into());
        }
        if header.magic != raw::PPDBCACHE_MAGIC {
            return Err(CacheErrorKind::InvalidMagic(header.magic).into());
        }

        if header.version != PPDBCACHE_VERSION {
            return Err(CacheErrorKind::WrongVersion(header.version).into());
        }

        let rest = align_buf(rest);

        let (lv, rest) = LayoutVerified::<_, [raw::SourceLocation]>::new_slice_from_prefix(
            rest,
            header.num_ranges as usize,
        )
        .ok_or(CacheErrorKind::InvalidSourceLocations)?;

        let source_locations = lv.into_slice();
        let rest = align_buf(rest);

        let (lv, rest) = LayoutVerified::<_, [raw::Range]>::new_slice_from_prefix(
            rest,
            header.num_ranges as usize,
        )
        .ok_or(CacheErrorKind::InvalidRanges)?;

        let ranges = lv.into_slice();
        let rest = align_buf(rest);

        if rest.len() < header.string_bytes as usize {
            return Err(CacheErrorKind::UnexpectedStringBytes {
                expected: header.string_bytes as usize,
                found: rest.len(),
            }
            .into());
        }

        Ok(Self {
            header,
            source_locations,
            ranges,
            string_bytes: rest,
        })
    }
}

fn align_buf(buf: &[u8]) -> &[u8] {
    let offset = buf.as_ptr().align_offset(8);
    buf.get(offset..).unwrap_or(&[])
}
