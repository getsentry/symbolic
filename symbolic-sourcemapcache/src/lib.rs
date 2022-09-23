//! A fast lookup cache for SourceMaps.

#![warn(missing_docs)]
#![allow(clippy::derive_partial_eq_without_eq)]

mod lookup;
mod raw;
mod writer;

pub use js_source_scopes::{ScopeLookupResult, SourcePosition};
pub use lookup::{Error as SourceMapCacheError, *};
pub use writer::*;
