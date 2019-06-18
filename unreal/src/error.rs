use failure::Fail;

/// Errors related to parsing an UE4 crash file.
#[derive(Fail, Debug)]
pub enum Unreal4Error {
    /// Expected UnrealEngine4 crash (zlib compressed).
    #[fail(display = "unknown bytes format")]
    UnknownBytesFormat,

    /// Empty data blob received.
    #[fail(display = "empty crash")]
    Empty,

    /// Value out of bounds.
    #[fail(display = "out of bounds")]
    OutOfBounds,

    /// Invalid compressed data.
    #[fail(display = "bad compression")]
    BadCompression(std::io::Error),

    /// Invalid contents of the crash file container.
    #[fail(display = "invalid crash file contents")]
    BadData(scroll::Error),

    /// The crash file contains unexpected trailing data after the footer.
    #[fail(display = "unexpected trailing data")]
    TrailingData,

    /// Can't process log entry.
    #[fail(display = "invalid log entry")]
    InvalidLogEntry(std::str::Utf8Error),

    /// Invalid XML
    #[fail(display = "invalid xml")]
    InvalidXml(elementtree::Error),
}

impl From<scroll::Error> for Unreal4Error {
    fn from(error: scroll::Error) -> Self {
        Unreal4Error::BadData(error)
    }
}
