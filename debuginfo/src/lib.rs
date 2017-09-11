//! Abstraction for reading debug info files.

extern crate uuid;
extern crate goblin;
extern crate gimli;
extern crate memmap;
extern crate scroll;
extern crate symbolic_common;

mod object;
mod dwarf;

pub use object::*;
pub use dwarf::*;
