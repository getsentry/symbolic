use std::fmt;

use failure::{Backtrace, Context, Fail};
use gimli;
use symbolic_debuginfo::ObjectError;

/// An internal error thrown during symcache conversion.
///
/// This error is used as cause for `BadDebugFile` errors to add more information to the generic
/// error kind. It should not be exposed to the user.
#[derive(Debug, Fail, Copy, Clone)]
#[fail(display = "{}", _0)]
pub(crate) struct ConversionError(pub &'static str);

/// Variants of `SymCacheError`.
#[derive(Debug, Fail, Copy, Clone, Eq, PartialEq)]
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

    /// A value cannot be written to symcache as it overflows the data format.
    #[fail(display = "value too large for symcache file format")]
    ValueTooLarge,

    /// Generic error when writing a symcache, most likely IO.
    #[fail(display = "failed to write symcache")]
    WriteFailed,
}

/// An error returned when handling symcaches.
#[derive(Debug)]
pub struct SymCacheError {
    inner: Context<SymCacheErrorKind>,
}

impl Fail for SymCacheError {
    fn cause(&self) -> Option<&Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl fmt::Display for SymCacheError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl SymCacheError {
    /// Returns the error kind of this error.
    pub fn kind(&self) -> SymCacheErrorKind {
        *self.inner.get_context()
    }
}

impl From<SymCacheErrorKind> for SymCacheError {
    fn from(kind: SymCacheErrorKind) -> SymCacheError {
        SymCacheError {
            inner: Context::new(kind),
        }
    }
}

impl From<Context<SymCacheErrorKind>> for SymCacheError {
    fn from(inner: Context<SymCacheErrorKind>) -> SymCacheError {
        SymCacheError { inner }
    }
}

impl From<ObjectError> for SymCacheError {
    fn from(error: ObjectError) -> SymCacheError {
        error.context(SymCacheErrorKind::BadDebugFile).into()
    }
}

impl From<gimli::Error> for SymCacheError {
    fn from(error: gimli::Error) -> SymCacheError {
        error.context(SymCacheErrorKind::BadDebugFile).into()
    }
}

impl From<ConversionError> for SymCacheError {
    fn from(error: ConversionError) -> SymCacheError {
        error.context(SymCacheErrorKind::BadDebugFile).into()
    }
}
