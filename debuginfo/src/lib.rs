//! Abstraction for reading debug info files.

extern crate goblin;
#[macro_use]
extern crate lazy_static;
extern crate regex;
#[cfg(feature = "with_serde")]
extern crate serde;
#[macro_use]
#[cfg(feature = "with_serde")]
extern crate serde_plain;
extern crate symbolic_common;
extern crate uuid;

mod breakpad;
mod dwarf;
mod elf;
mod id;
mod mach;
mod object;
mod symbols;

pub use breakpad::*;
pub use dwarf::*;
pub use id::*;
pub use object::*;
pub use symbols::*;
