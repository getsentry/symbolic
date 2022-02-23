//! Provides SymCache support.

#![warn(missing_docs)]

mod compat;
mod new;
mod old;
pub(crate) mod preamble;

pub use compat::*;
pub use new::transform;
pub use new::SymCacheWriter;
#[allow(deprecated)]
pub use old::format;
pub use old::{Line, LineInfo, SymCacheError, SymCacheErrorKind, ValueKind};

/// The latest version of the file format.
pub const SYMCACHE_VERSION: u32 = 7;

// Version history:
//
// 1: Initial implementation
// 2: PR #58:  Migrate from UUID to Debug ID
// 3: PR #148: Consider all PT_LOAD segments in ELF
// 4: PR #155: Functions with more than 65k line records
// 5: PR #221: Invalid inlinee nesting leading to wrong stack traces
// 6: PR #319: Correct line offsets and spacer line records
// 7: PR #459: A new binary format fundamentally based on addr ranges
