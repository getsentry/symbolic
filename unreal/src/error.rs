use failure::Fail;

/// Errors related to parsing an UE4 crash file.
#[non_exhaustive]
#[derive(Fail, Debug)]
pub enum Unreal4Error {
    /// Empty data blob received.
    #[fail(display = "empty crash")]
    Empty,

    /// Invalid compressed data.
    #[fail(display = "bad compression")]
    BadCompression(std::io::Error),

    /// Invalid contents of the crash file container.
    #[fail(display = "invalid crash file contents")]
    BadData(scroll::Error),

    /// The crash file contains unexpected trailing data after the footer.
    #[fail(display = "unexpected trailing data")]
    TrailingData,

    /// Can't process a log entry.
    #[fail(display = "invalid log entry")]
    InvalidLogEntry(std::str::Utf8Error),

    /// Invalid XML.
    #[fail(display = "invalid xml")]
    InvalidXml(elementtree::Error),
}

impl From<scroll::Error> for Unreal4Error {
    fn from(error: scroll::Error) -> Self {
        Unreal4Error::BadData(error)
    }
}
