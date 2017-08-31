use std::io;

error_chain! {
    errors {
        BadSymbol(message: String) {
            description("bad symbol")
            display("bad symbol: {}", &message)
        }
    }

    foreign_links {
        Io(io::Error);
    }
}
