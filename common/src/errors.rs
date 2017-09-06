use std::io;

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
    }
}
