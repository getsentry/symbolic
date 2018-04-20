use std::fmt;

use failure::{Backtrace, Context, Fail};
use gimli;
use symbolic_debuginfo::ObjectError;

#[derive(Debug, Fail, Copy, Clone)]
#[fail(display = "{}", _0)]
pub(crate) struct ConversionError(pub &'static str);

#[derive(Debug, Fail, Copy, Clone, Eq, PartialEq)]
pub enum SymCacheErrorKind {
    #[fail(display = "bad symcache magic")]
    BadFileMagic,
    #[fail(display = "invalid symcache header")]
    BadFileHeader,
    #[fail(display = "cannot read symcache segment")]
    BadSegment,
    #[fail(display = "malformed symcache file")]
    BadCacheFile,
    #[fail(display = "unsupported symcache version")]
    UnsupportedVersion,
    #[fail(display = "malformed debug info file")]
    BadDebugFile,
    #[fail(display = "missing debug section")]
    MissingDebugSection,
    #[fail(display = "no debug information found in file")]
    MissingDebugInfo,
    #[fail(display = "unsupported debug information")]
    UnsupportedDebugKind,
    #[fail(display = "value too large for symcache file format")]
    ValueTooLarge,
    #[fail(display = "failed to write symcache")]
    WriteFailed,
}

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
