//! Abstraction for reading debug info files.

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
