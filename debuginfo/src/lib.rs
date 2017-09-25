//! Abstraction for reading debug info files.

extern crate uuid;
extern crate goblin;
extern crate gimli;
extern crate memmap;
extern crate scroll;
#[macro_use] extern crate symbolic_common;
#[macro_use] extern crate if_chain;

mod object;
mod dwarf;

pub use object::*;
pub use dwarf::*;
