//! Symbolic works with symbols and debug info.
//!
//! This library implements various utilities to help Sentry
//! symbolicate stacktraces.  It is built to also be used independently
//! of Sentry and in parts.

#[doc(hidden)]
pub extern crate symbolic_common;
#[doc(hidden)]
#[cfg(feature = "debuginfo")]
pub extern crate symbolic_debuginfo;
#[doc(hidden)]
#[cfg(feature = "demangle")]
pub extern crate symbolic_demangle;
#[doc(hidden)]
#[cfg(feature = "minidump")]
pub extern crate symbolic_minidump;
#[doc(hidden)]
#[cfg(feature = "proguard")]
pub extern crate symbolic_proguard;
#[doc(hidden)]
#[cfg(feature = "sourcemap")]
pub extern crate symbolic_sourcemap;
#[doc(hidden)]
#[cfg(feature = "symcache")]
pub extern crate symbolic_symcache;

pub use symbolic_common as common;
#[cfg(feature = "debuginfo")]
pub use symbolic_debuginfo as debuginfo;
#[cfg(feature = "demangle")]
pub use symbolic_demangle as demangle;
#[cfg(feature = "minidump")]
pub use symbolic_minidump as minidump;
#[cfg(feature = "proguard")]
pub use symbolic_proguard as proguard;
#[cfg(feature = "sourcemap")]
pub use symbolic_sourcemap as sourcemap;
#[cfg(feature = "symcache")]
pub use symbolic_symcache as symcache;
