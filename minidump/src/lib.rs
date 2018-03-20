//! Provides minidump support.
extern crate gimli;
#[macro_use]
extern crate lazy_static;
extern crate regex;
#[cfg(feature = "with_serde")]
extern crate serde;
#[macro_use]
#[cfg(feature = "with_serde")]
extern crate serde_plain;
extern crate uuid;

extern crate symbolic_common;
extern crate symbolic_debuginfo;

mod cfi;
mod processor;
mod registers;
mod utils;

pub use cfi::*;
pub use processor::*;
