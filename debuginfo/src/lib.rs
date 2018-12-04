//! Abstraction for reading debug info files.

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate failure_derive;

extern crate failure;
extern crate flate2;
extern crate goblin;
extern crate regex;
extern crate symbolic_common;
extern crate uuid;

mod breakpad;
mod dwarf;
mod elf;
mod features;
mod mach;
mod object;
mod symbols;

pub use crate::breakpad::*;
pub use crate::dwarf::*;
pub use crate::features::*;
pub use crate::object::*;
pub use crate::symbols::*;

#[deprecated]
pub use symbolic_common::types::{BreakpadFormat, DebugId, ParseDebugIdError};
