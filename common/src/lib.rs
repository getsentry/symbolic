//! Common functionality for symbolic.
//!
//! In particular this defines common error types and similar things
//! that all symbolic crates want to use.
#[macro_use] extern crate error_chain;

mod errors;

pub use errors::*;
