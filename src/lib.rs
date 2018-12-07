//! Symbolic works with symbols and debug info.
//!
//! This library implements various utilities to help Sentry
//! symbolicate stacktraces.  It is built to also be used independently
//! of Sentry and in parts.

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
#[cfg(feature = "unreal")]
pub use symbolic_unreal as unreal;
