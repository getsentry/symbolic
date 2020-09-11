use thiserror::Error;

/// Errors related to parsing an UE4 crash file.
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum Unreal4Error {
    /// Empty data blob received.
    #[error("empty crash")]
    Empty,

    /// Invalid compressed data.
    #[error("bad compression")]
    BadCompression(#[source] std::io::Error),

    /// Invalid contents of the crash file container.
    #[error("invalid crash file contents")]
    BadData(#[from] scroll::Error),

    /// The crash file contains unexpected trailing data after the footer.
    #[error("unexpected trailing data")]
    TrailingData,

    /// Can't process a log entry.
    #[error("invalid log entry")]
    InvalidLogEntry(#[from] std::str::Utf8Error),

    /// Invalid XML.
    #[error("invalid xml")]
    InvalidXml(#[from] elementtree::Error),
}
