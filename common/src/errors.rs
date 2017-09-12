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
        UnknownCacheFileVersion(version: u32) {
            description("unknown cache file version")
            display("unknown cache file version '{}'", version)
        }
    }

    foreign_links {
        IoError(::std::io::Error);
        Utf8Error(::std::str::Utf8Error);
        GoblinError(::goblin::error::Error);
        GimliError(::gimli::Error);
        ScrollError(::scroll::Error);
    }
}
