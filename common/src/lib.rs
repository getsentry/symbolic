//! Common functionality for symbolic.
//!
//! In particular this defines common error types and similar things
//! that all symbolic crates want to use.
#![recursion_limit = "128"]

#[macro_use]
extern crate error_chain;
#[cfg(feature = "with_dwarf")]
extern crate gimli;
#[cfg(feature = "with_objects")]
extern crate goblin;
#[cfg(feature = "with_objects")]
extern crate scroll;
extern crate memmap;
extern crate owning_ref;

mod macros;
mod errors;
mod types;
mod byteview;

pub use errors::*;
pub use types::*;
pub use byteview::*;
