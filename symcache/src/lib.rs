//! Provides symcache support.
extern crate dmsort;
extern crate fallible_iterator;
extern crate fnv;
extern crate gimli;
#[macro_use]
extern crate if_chain;
extern crate lru_cache;
#[macro_use]
extern crate matches;
extern crate num;
#[macro_use]
extern crate symbolic_common;
extern crate symbolic_debuginfo;
extern crate symbolic_demangle;
extern crate uuid;

mod breakpad;
mod dwarf;
mod types;
mod cache;
mod writer;
mod heuristics;
mod utils;

pub use types::DataSource;
pub use cache::*;
pub use writer::*;
pub use heuristics::*;
