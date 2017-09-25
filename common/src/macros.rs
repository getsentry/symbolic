#[macro_export]
macro_rules! itry {
    ($expr:expr) => {
        match $expr {
            Ok(rv) => rv,
            Err(err) => {
                return Some(Err(::std::convert::From::from(err)));
            }
        }
    }
}
