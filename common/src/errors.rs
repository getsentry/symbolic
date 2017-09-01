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
    }

    foreign_links {
        Io(io::Error);
    }
}
