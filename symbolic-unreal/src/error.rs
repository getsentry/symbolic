use std::error::Error;
use std::fmt;

use thiserror::Error;

/// Errors related to parsing an UE4 crash file.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unreal4ErrorKind {
    /// Empty data blob received.
    Empty,

    /// Invalid compressed data.
    BadCompression,

    /// Invalid contents of the crash file container.
    BadData,

    /// The crash file contains unexpected trailing data after the footer.
    TrailingData,

    /// Can't process a log entry.
    InvalidLogEntry,

    /// Invalid XML.
    InvalidXml,
}

impl fmt::Display for Unreal4ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty crash"),
            Self::BadCompression => write!(f, "bad compression"),
            Self::BadData => write!(f, "invalid crash file contents"),
            Self::TrailingData => write!(f, "unexpected trailing data"),
            Self::InvalidLogEntry => write!(f, "invalid log entry"),
            Self::InvalidXml => write!(f, "invalid xml"),
        }
    }
}

/// An error returned when handling an UE4 crash file.
#[derive(Debug, Error)]
#[error("{kind}")]
pub struct Unreal4Error {
    kind: Unreal4ErrorKind,
    #[source]
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl Unreal4Error {
    /// Creates a new Unreal4 error from a known kind of error as well as an
    /// arbitrary error payload.
    ///
    /// This function is used to generically create Unreal4 errors which do not
    /// originate from `symbolic` itself. The `source` argument is an arbitrary
    /// payload which will be contained in this [`Unreal4Error`].
    pub fn new<E>(kind: Unreal4ErrorKind, source: E) -> Self
    where
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        let source = Some(source.into());
        Self { kind, source }
    }

    /// Returns the corresponding [`Unreal4ErrorKind`] for this error.
    pub fn kind(&self) -> Unreal4ErrorKind {
        self.kind
    }
}

impl From<Unreal4ErrorKind> for Unreal4Error {
    fn from(kind: Unreal4ErrorKind) -> Self {
        Self { kind, source: None }
    }
}

impl From<elementtree::Error> for Unreal4Error {
    fn from(source: elementtree::Error) -> Self {
        Self::new(Unreal4ErrorKind::InvalidXml, source)
    }
}

impl From<scroll::Error> for Unreal4Error {
    fn from(source: scroll::Error) -> Self {
        Self::new(Unreal4ErrorKind::BadData, source)
    }
}
