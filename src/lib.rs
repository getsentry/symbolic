//! Symbolic works with symbols and debug info.
//!
//! This library implements various utilities to help Sentry
//! symbolicate stacktraces.  It is built to also be used independently
//! of Sentry and in parts.

#[doc(hidden)] pub extern crate symbolic_proguard;
#[doc(hidden)] pub extern crate symbolic_sourcemap;
#[doc(hidden)] pub extern crate symbolic_demangle;
#[doc(hidden)] pub extern crate symbolic_minidump;
#[doc(hidden)] pub extern crate symbolic_symcache;
#[doc(hidden)] pub extern crate symbolic_common;
#[doc(hidden)] pub extern crate symbolic_debuginfo;

pub use symbolic_proguard as proguard;
pub use symbolic_proguard as sourcemap;
pub use symbolic_demangle as demangle;
pub use symbolic_minidump as minidump;
pub use symbolic_symcache as symcache;
pub use symbolic_debuginfo as debuginfo;
pub use symbolic_common as common;

pub use common::{Error, Result, ErrorKind, ResultExt};
