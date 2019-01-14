//! Common functionality for symbolic.
//!
//! In particular this defines common error types and similar things
//! that all symbolic crates want to use.

pub mod byteview;
pub mod types;

/// We export our gimli dependency out of this crate so other scan use it.  This is just
/// so that we have a consistent use of it.
#[doc(hidden)]
pub use gimli as shared_gimli;
