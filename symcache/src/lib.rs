//! Provides symcache support.
#[macro_use] extern crate symbolic_common;
extern crate symbolic_debuginfo;
extern crate symbolic_demangle;
extern crate uuid;
extern crate gimli;
extern crate fallible_iterator;
extern crate lru_cache;
extern crate fnv;
extern crate num;
#[macro_use] extern crate matches;
#[macro_use] extern crate if_chain;

mod types;
mod cache;
mod writer;
mod heuristics;
mod utils;

pub use types::DataSource;
pub use cache::*;
pub use writer::*;
pub use heuristics::*;
