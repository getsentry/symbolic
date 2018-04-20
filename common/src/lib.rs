//! Common functionality for symbolic.
//!
//! In particular this defines common error types and similar things
//! that all symbolic crates want to use.
#![recursion_limit = "128"]

// #[macro_use]
// extern crate error_chain;
extern crate failure;
#[macro_use]
extern crate failure_derive;
#[cfg(feature = "with_dwarf")]
extern crate gimli;
#[cfg(feature = "with_objects")]
extern crate goblin;
extern crate memmap;
extern crate owning_ref;
#[cfg(feature = "with_serde")]
extern crate serde;
#[macro_use]
#[cfg(feature = "with_serde")]
extern crate serde_plain;

pub mod types;
pub mod byteview;
