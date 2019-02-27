/// Defines an error type with a failure context and a kind.
///
/// The kind enum needs to be defined explicitly, and has to implement `Fail` already. This macro
/// then defines an error type that wraps the kind in a `failure::Context` which makes it carry a
/// backtrace and allows nesting errors, and also defines the following methods on the error type:
///
///  - `kind()`: Returns a reference to the error kind.
///  - `cause()`: Returns the causing error, if any.
///  - `backtrace()`: Returns the backtrace of this error (or the cause), if
///    `RUST_BACKTRACE` was set.
///
/// In addition to this, the following conversions are defined for the error type:
///  - `From<ErrorKind>`
///  - `From<Context<ErrorKind>>`
///
/// ## Example
///
/// ```rust
/// # use symbolic_common::derive_failure;
/// enum MyErrorKind {
///     Something,
///     Else,
/// }
///
/// derive_failure!(MyError, MyErrorKind);
///
/// fn something() -> Result<(), MyError> {
///     Err(ErrorKind::Something.into())
/// }
/// ```
#[macro_export]
macro_rules! derive_failure {
    ($error:ident, $kind:ident $(, $meta:meta)* $(,)?) => {
        $(#[$meta])*
        #[derive(Debug)]
        pub struct $error {
            inner: ::failure::Context<$kind>,
        }

        impl ::failure::Fail for $error {
            fn cause(&self) -> Option<&dyn ::failure::Fail> {
                self.inner.cause()
            }

            fn backtrace(&self) -> Option<&::failure::Backtrace> {
                self.inner.backtrace()
            }
        }

        impl ::std::fmt::Display for $error {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                self.inner.fmt(f)
            }
        }

        impl $error {
            /// Returns the error kind of this error.
            pub fn kind(&self) -> &$kind {
                &self.inner.get_context()
            }
        }

        impl From<$kind> for $error {
            fn from(kind: $kind) -> Self {
                $error {
                    inner: ::failure::Context::new(kind),
                }
            }
        }

        impl From<::failure::Context<$kind>> for $error {
            fn from(inner: ::failure::Context<$kind>) -> Self {
                $error { inner }
            }
        }
    };
}
