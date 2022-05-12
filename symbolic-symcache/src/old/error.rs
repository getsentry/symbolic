use std::error::Error;
use std::fmt;

use thiserror::Error;

#[doc(hidden)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ValueKind {
    Symbol,
    Function,
    File,
    Line,
    ParentOffset,
    Language,
}

impl fmt::Display for ValueKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ValueKind::Symbol => write!(f, "symbol"),
            ValueKind::Function => write!(f, "function"),
            ValueKind::File => write!(f, "file"),
            ValueKind::Line => write!(f, "line record"),
            ValueKind::ParentOffset => write!(f, "inline parent offset"),
            ValueKind::Language => write!(f, "language"),
        }
    }
}

/// The error type for [`SymCacheError`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymCacheErrorKind {
    /// Invalid magic bytes in the symcache header.
    BadFileMagic,

    /// Invalid flags or fields in the symcache header.
    BadFileHeader,

    /// A segment could not be read, likely due to IO errors.
    BadSegment,

    /// Contents in the symcache file are malformed.
    BadCacheFile,

    /// The symcache version is not known.
    UnsupportedVersion,

    /// The `Object` contains invalid data and cannot be converted.
    BadDebugFile,

    /// A required debug section is missing in the `Object` file.
    MissingDebugSection,

    /// The `Object` file was stripped of debug information.
    MissingDebugInfo,

    /// The debug information in the `Object` file is not supported.
    UnsupportedDebugKind,

    /// A value cannot be written to symcache as it overflows the record size.
    ValueTooLarge(ValueKind),

    /// A value cannot be written to symcache as it overflows the segment counter.
    TooManyValues(ValueKind),

    /// Generic error when writing a symcache, most likely IO.
    WriteFailed,
}

impl fmt::Display for SymCacheErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadFileMagic => write!(f, "bad symcache magic"),
            Self::BadFileHeader => write!(f, "invalid symcache header"),
            Self::BadSegment => write!(f, "cannot read symcache segment"),
            Self::BadCacheFile => write!(f, "malformed symcache file"),
            Self::UnsupportedVersion => write!(f, "unsupported symcache version"),
            Self::BadDebugFile => write!(f, "malformed debug info file"),
            Self::MissingDebugSection => write!(f, "missing debug section"),
            Self::MissingDebugInfo => write!(f, "no debug information found in file"),
            Self::UnsupportedDebugKind => write!(f, "unsupported debug information"),
            Self::ValueTooLarge(kind) => write!(f, "{} too large for symcache file format", kind),
            Self::TooManyValues(kind) => write!(f, "too many {}s for symcache", kind),
            Self::WriteFailed => write!(f, "failed to write symcache"),
        }
    }
}

/// An error returned when handling a [`SymCache`](crate::SymCache).
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct SymCacheError {
    pub(crate) kind: SymCacheErrorKind,
    #[source]
    pub(crate) source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl SymCacheError {
    /// Creates a new SymCache error from a known kind of error as well as an
    /// arbitrary error payload.
    pub(crate) fn new<E>(kind: SymCacheErrorKind, source: E) -> Self
    where
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        let source = Some(source.into());
        Self { kind, source }
    }

    /// Returns the corresponding [`SymCacheErrorKind`] for this error.
    pub fn kind(&self) -> SymCacheErrorKind {
        self.kind
    }
}

impl From<SymCacheErrorKind> for SymCacheError {
    fn from(kind: SymCacheErrorKind) -> Self {
        Self { kind, source: None }
    }
}
