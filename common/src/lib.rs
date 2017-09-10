//! Common functionality for symbolic.
//!
//! In particular this defines common error types and similar things
//! that all symbolic crates want to use.
#[macro_use] extern crate error_chain;
extern crate goblin;
extern crate scroll;
extern crate memmap;

mod errors;
mod types;
mod byteview;

pub use errors::*;
pub use types::*;
pub use byteview::*;
