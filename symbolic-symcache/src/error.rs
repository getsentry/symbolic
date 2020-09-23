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

/// An error returned when handling [`SymCache`](struct.SymCache.html).
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum SymCacheError {
    /// Invalid magic bytes in the symcache header.
    #[error("bad symcache magic")]
    BadFileMagic,

    /// Invalid flags or fields in the symcache header.
    #[error("invalid symcache header")]
    BadFileHeader(#[source] std::io::Error),

    /// A segment could not be read, likely due to IO errors.
    #[error("cannot read symcache segment")]
    BadSegment,

    /// Contents in the symcache file are malformed.
    #[error("malformed symcache file")]
    BadCacheFile,

    /// The symcache version is not known.
    #[error("unsupported symcache version")]
    UnsupportedVersion,

    /// The `Object` contains invalid data and cannot be converted.
    #[error("malformed debug info file")]
    BadDebugFile(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),

    /// A required debug section is missing in the `Object` file.
    #[error("missing debug section")]
    MissingDebugSection,

    /// The `Object` file was stripped of debug information.
    #[error("no debug information found in file")]
    MissingDebugInfo,

    /// The debug information in the `Object` file is not supported.
    #[error("unsupported debug information")]
    UnsupportedDebugKind,

    /// A value cannot be written to symcache as it overflows the record size.
    #[error("{0} too large for symcache file format")]
    ValueTooLarge(ValueKind),

    /// A value cannot be written to symcache as it overflows the segment counter.
    #[error("too many {0}s for symcache")]
    TooManyValues(ValueKind),

    /// Generic error when writing a symcache, most likely IO.
    #[error("failed to write symcache")]
    WriteFailed(#[source] std::io::Error),
}
