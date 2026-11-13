//! API to process Unreal Engine 4 crashes.
#![warn(missing_docs)]
// fix(SYMBOL-37): This is a false positive triggered in derive macros.
// See <https://github.com/getsentry/symbolic/issues/1086>.
#![allow(clippy::redundant_field_names)]

mod container;
mod context;
mod error;
mod logs;
mod xml;

pub use container::*;
pub use context::*;
pub use error::*;
pub use logs::*;
