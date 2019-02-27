//! Abstractions for dealing with object files and debug information.
//!
//! This crate defines the [`Object`] type, which is an abstraction over various object file formats
//! used in different platforms. Also, since executables on MacOS might contain multiple object
//! files (called a _"Fat MachO"_), there is an [`Archive`] type, that provides a uniform interface
//! with access to an objects iterator in all platforms.
//!
//! Most processing of object files will happen on the `Object` type or its concrete implementation
//! for one platform. To allow abstraction over this, there is the [`ObjectLike`] trait. It defines
//! common attributes and gives access to a [`DebugSession`], which can be used to perform more
//! stateful handling of debug information.
//!
//! See [`Object`] for the full API, or use one of the modules for direct access to the
//! platform-dependent data.
//!
//! [`Object`]: enum.Object.html
//! [`Archive`]: enum.Archive.html
//! [`ObjectLike`]: trait.ObjectLike.html
//! [`DebugSession`]: trait.DebugSession.html

#![warn(missing_docs)]

mod base;
mod object;
mod private;

pub mod breakpad;
pub mod dwarf;
pub mod elf;
pub mod macho;
pub mod pdb;
pub mod pe;

pub use crate::base::*;
pub use crate::object::*;
