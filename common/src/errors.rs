use std::io;
use std::str;
use std::ffi;

#[cfg(feature = "with_dwarf")]
use gimli;
#[cfg(feature = "with_objects")]
use goblin;
#[cfg(feature = "with_objects")]
use scroll;

error_chain! {
    errors {
        /// Raised in some cases if panics are caught
        Panic(message: String) {
            description("panic")
            display("panic: {}", message)
        }
        /// Raised on operations that work on symbols if the symbol
        /// data is bad.
        BadSymbol(message: String) {
            description("bad symbol")
            display("bad symbol: {}", &message)
        }
        /// Raised for internal errors in the libraries.  Should not happen.
        Internal(message: &'static str) {
            description("internal error")
            display("internal error: {}", message)
        }
        /// Raised for bad input on parsing in symbolic.
        Parse(message: &'static str) {
            description("parse error")
            display("parse error: {}", message)
        }
        /// General error for missing information.
        NotFound(message: &'static str) {
            description("not found")
            display("not found: {}", message)
        }
        /// Raised for bad input on parsing in symbolic.
        Format(message: &'static str) {
            description("format error")
            display("format error: {}", message)
        }
        /// Raised if unsupported object files are loaded.
        UnsupportedObjectFile {
            description("unsupported object file")
        }
        /// Raised if object files are malformed.
        MalformedObjectFile(message: String) {
            description("malformed object file")
            display("malformed object file: {}", &message)
        }
        /// Raised for unknown cache file versions.
        BadCacheFile(msg: &'static str) {
            description("bad cache file")
            display("bad cache file: {}", msg)
        }
        /// Raised if a section is missing in an object file.
        MissingSection(section: &'static str) {
            description("missing object section")
            display("missing object section '{}'", section)
        }
        /// Raised for DWARF failures.
        BadDwarfData(message: &'static str) {
            description("bad dwarf data")
            display("bad dwarf data: {}", message)
        }
        MissingDebugInfo(message: &'static str) {
            description("missing debug info")
            display("missing debug info: {}", message)
        }
        /// Raised while stackwalking minidumps.
        Stackwalk(message: String) {
            description("stackwalking error")
            display("stackwalking error: {}", message)
        }
        /// Raised when a resolver cannot load its symbols file.
        Resolver(message: String) {
            description("resolver error")
            display("resolver error: {}", message)
        }
    }

    foreign_links {
        Io(io::Error);
        Utf8Error(str::Utf8Error);
        ParseInt(::std::num::ParseIntError);
    }
}

#[cfg(feature = "with_dwarf")]
impl From<gimli::Error> for Error {
    fn from(err: gimli::Error) -> Error {
        use std::mem;
        use std::error::Error;
        // this works because gimli error only returns static strings. UUUGLY
        ErrorKind::BadDwarfData(unsafe { mem::transmute(err.description()) }).into()
    }
}

#[cfg(feature = "with_objects")]
impl From<goblin::error::Error> for Error {
    fn from(err: goblin::error::Error) -> Error {
        use goblin::error::Error::*;
        match err {
            Malformed(s) => ErrorKind::MalformedObjectFile(s).into(),
            BadMagic(m) => ErrorKind::MalformedObjectFile(format!("bad magic: {}", m)).into(),
            Scroll(err) => Error::from(err),
            IO(err) => Error::from(err),
        }
    }
}

#[cfg(feature = "with_objects")]
impl From<scroll::Error> for Error {
    fn from(err: scroll::Error) -> Error {
        use scroll::Error::*;
        match err {
            TooBig { .. } => io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Tried to read type that was too large",
            ).into(),
            BadOffset(..) => io::Error::new(io::ErrorKind::InvalidData, "Bad offset").into(),
            BadInput { .. } => io::Error::new(io::ErrorKind::InvalidData, "Bad input").into(),
            Custom(s) => io::Error::new(io::ErrorKind::Other, s).into(),
            IO(err) => Error::from(err),
        }
    }
}

impl From<ffi::NulError> for Error {
    fn from(_err: ffi::NulError) -> Error {
        ErrorKind::Internal("unexpected null byte in c-string").into()
    }
}
