//! Provides symcache support.
extern crate symbolic_common;
extern crate symbolic_debuginfo;
extern crate uuid;
extern crate gimli;
extern crate fallible_iterator;


mod types;
mod cache;
mod writer;
mod utils;

pub use cache::{Symbol, SymCache};
pub use writer::SymCacheWriter;
