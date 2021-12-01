//! Provides SymCache support.

#![warn(missing_docs)]

mod compat;
mod new;
mod old;
pub(crate) mod preamble;

pub use compat::*;
pub use new::raw::SYMCACHE_VERSION;
pub use new::SymCacheWriter;
pub use old::{format, Line, LineInfo, SymCacheError, SymCacheErrorKind, ValueKind};
