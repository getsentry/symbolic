//! Symbolic works with symbols and debug info.
//!
//! This library implements various utilities to help Sentry
//! symbolicate stacktraces.  It is built to also be used independently
//! of Sentry and in parts.

#![warn(missing_docs)]

#[doc(inline)]
pub use symbolic_common as common;
#[doc(inline)]
#[cfg(feature = "debuginfo")]
pub use symbolic_debuginfo as debuginfo;
#[doc(inline)]
#[cfg(feature = "demangle")]
pub use symbolic_demangle as demangle;
#[doc(inline)]
#[cfg(feature = "minidump")]
pub use symbolic_minidump as minidump;
#[doc(inline)]
#[cfg(feature = "proguard")]
#[deprecated = "use the `proguard` crate directly"]
pub use symbolic_proguard as proguard;
#[doc(inline)]
#[cfg(feature = "sourcemap")]
pub use symbolic_sourcemap as sourcemap;
#[doc(inline)]
#[cfg(feature = "symcache")]
pub use symbolic_symcache as symcache;
#[doc(inline)]
#[cfg(feature = "unreal")]
pub use symbolic_unreal as unreal;
