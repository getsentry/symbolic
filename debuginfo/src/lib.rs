//! Abstraction for reading debug info files.

extern crate uuid;
extern crate goblin;
#[macro_use] extern crate symbolic_common;
#[macro_use] extern crate if_chain;

mod breakpad;
mod object;
mod dwarf;

pub use breakpad::*;
pub use object::*;
pub use dwarf::*;
