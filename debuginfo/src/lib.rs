//! Abstraction for reading debug info files.

extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate goblin;
#[macro_use]
extern crate lazy_static;
extern crate regex;
// #[cfg(feature = "with_serde")]
// extern crate serde;
extern crate symbolic_common;
extern crate uuid;

mod breakpad;
mod dwarf;
mod elf;
mod mach;
mod object;
mod symbols;

pub use breakpad::*;
pub use dwarf::*;
pub use object::*;
pub use symbols::*;
#[deprecated]
pub use symbolic_common::types::{BreakpadFormat, DebugId, ParseDebugIdError};
