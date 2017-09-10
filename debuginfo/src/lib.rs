//! Abstraction for reading debug info files.

extern crate goblin;
extern crate gimli;
extern crate memmap;
extern crate scroll;
extern crate symbolic_common;

mod object;

pub use object::*;
