use std::fmt;

use failure::Fail;

use symbolic_common::derive_failure;
use symbolic_debuginfo::ObjectError;

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

/// Variants of `SymCacheError`.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Fail, PartialEq)]
pub enum SymCacheErrorKind {
    /// Invalid magic bytes in the symcache header.
    #[fail(display = "bad symcache magic")]
    BadFileMagic,

    /// Invalid flags or fields in the symcache header.
    #[fail(display = "invalid symcache header")]
    BadFileHeader,

    /// A segment could not be read, likely due to IO errors.
    #[fail(display = "cannot read symcache segment")]
    BadSegment,

    /// Contents in the symcache file are malformed.
    #[fail(display = "malformed symcache file")]
    BadCacheFile,

    /// The symcache version is not known.
    #[fail(display = "unsupported symcache version")]
    UnsupportedVersion,

    /// The `Object` contains invalid data and cannot be converted.
    #[fail(display = "malformed debug info file")]
    BadDebugFile,

    /// A required debug section is missing in the `Object` file.
    #[fail(display = "missing debug section")]
    MissingDebugSection,

    /// The `Object` file was stripped of debug information.
    #[fail(display = "no debug information found in file")]
    MissingDebugInfo,

    /// The debug information in the `Object` file is not supported.
    #[fail(display = "unsupported debug information")]
    UnsupportedDebugKind,

    /// A value cannot be written to symcache as it overflows the record size.
    #[fail(display = "{} too large for symcache file format", _0)]
    ValueTooLarge(ValueKind),

    /// A value cannot be written to symcache as it overflows the segment counter.
    #[fail(display = "too many {}s for symcache", _0)]
    TooManyValues(ValueKind),

    /// Generic error when writing a symcache, most likely IO.
    #[fail(display = "failed to write symcache")]
    WriteFailed,
}

derive_failure!(
    SymCacheError,
    SymCacheErrorKind,
    doc = "An error returned when handling `SymCaches`.",
);

impl From<ObjectError> for SymCacheError {
    fn from(error: ObjectError) -> SymCacheError {
        error.context(SymCacheErrorKind::BadDebugFile).into()
    }
}
