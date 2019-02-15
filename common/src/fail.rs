#[macro_export]
macro_rules! derive_failure {
    ($error:ident, $kind:ident) => {
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
