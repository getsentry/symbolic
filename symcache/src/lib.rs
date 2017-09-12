//! Provides symcache support.
extern crate symbolic_common;
extern crate uuid;
extern crate gimli;


mod types;
mod cache;
mod utils;

pub use cache::{Symbol, SymCache};
