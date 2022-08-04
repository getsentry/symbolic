//! Experimental IL2CPP code.
//!
//! This crate **is not supported**, it may break its API, it may completely disappear
//! again.  Do not consider this part of symbolic releases.  It is experimental code to
//! explore Unity IL2CPP debugging.

mod line_mapping;

pub use line_mapping::{LineMapping, ObjectLineMapping};
