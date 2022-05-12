//! Provides minidump support.

#![warn(missing_docs)]

#[cfg(feature = "processor")]
mod utils;

pub mod cfi;

#[cfg(feature = "processor")]
pub mod processor;
