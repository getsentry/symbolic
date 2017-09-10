use std::io;
use std::str;

use goblin;
use scroll;

error_chain! {
    errors {
        BadSymbol(message: String) {
            description("bad symbol")
            display("bad symbol: {}", &message)
        }
        InternalError(message: &'static str) {
            description("internal error")
            display("internal error: {}", &message)
        }
        ParseError(message: &'static str) {
            description("parse error")
            display("parse error: {}", &message)
        }

        UnsupportedObjectFile {
            description("unsupported object file")
        }
        MalformedObjectFile(msg: String) {
            description("malformed object file")
            display("malformed object file: {}", &msg)
        }
        CorruptCacheFile {
            description("corrupt cache file")
        }
        UnknownCacheFileVersion(version: u32) {
            description("unknown cache file version")
            display("unknown cache file version '{}'", version)
        }
    }

    foreign_links {
        Io(io::Error);
        Utf8Error(str::Utf8Error);
    }
}

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

impl From<scroll::Error> for Error {
    fn from(err: scroll::Error) -> Error {
        use scroll::Error::*;
        match err {
            TooBig { .. } => {
                io::Error::new(io::ErrorKind::Other, "Tried to read type that was too large").into()
            },
            BadOffset(..) => {
                io::Error::new(io::ErrorKind::Other, "Bad offset").into()
            },
            BadInput { .. } => {
                io::Error::new(io::ErrorKind::Other, "Bad input").into()
            }
            Custom(s) => {
                io::Error::new(io::ErrorKind::Other, s).into()
            }
            IO(err) => Error::from(err),
        }
    }
}
