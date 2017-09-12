//! Provides symcache support.
extern crate symbolic_common;
extern crate uuid;
extern crate gimli;


mod types;
mod read;
mod utils;

pub use read::{Symbol, SymCache};
