//! Common functionality for symbolic.
//!
//! In particular this defines common error types and similar things
//! that all symbolic crates want to use.

mod byteview;
mod cell;
mod fail;
mod heuristics;
mod path;
mod types;

pub use crate::byteview::*;
pub use crate::cell::*;
pub use crate::fail::*;
pub use crate::heuristics::*;
pub use crate::path::*;
pub use crate::types::*;

pub use debugid::*;
pub use uuid::Uuid;

// /// We export our gimli dependency out of this crate so other scan use it.  This is just
// /// so that we have a consistent use of it.
// #[doc(hidden)]
// pub use gimli as shared_gimli;
