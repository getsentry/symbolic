//! A library to bundle sources from debug files for later processing.
//!
//! TODO(jauer): Describe contents

#![warn(missing_docs)]

mod bundle;
mod error;

pub use crate::bundle::*;
pub use crate::error::*;
