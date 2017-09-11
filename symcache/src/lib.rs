//! Provides symcache support.
extern crate symbolic_common;
extern crate uuid;
extern crate memmap;


mod types;
mod read;
mod utils;

pub use read::{Symbol, SymCache};
