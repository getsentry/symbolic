//! Abstractions for dealing with object files and debug information.
//!
//! This module defines the [`Object`] type, which is an abstraction over various object file
//! formats used in different platforms. Also, since executables on MacOS might contain multiple
//! object files (called a _"Fat MachO"_), there is an [`Archive`] type, that provides a uniform
//! interface with access to an objects iterator in all platforms.
//!
//! Most processing of object files will happen on the `Object` type or its concrete implementation
//! for one platform. To allow abstraction over this, there is the [`ObjectLike`] trait. It defines
//! common attributes and gives access to a [`DebugSession`], which can be used to perform more
//! stateful handling of debug information.
//!
//! See [`Object`] for the full API, or use one of the modules for direct access to the
//! platform-dependent data.
//!
//! # Background
//!
//! The functionality of `symbolic::debuginfo` is conceptionally similar to the [`object`] crate.
//! However, there are key differences that warranted a separate implementation:
//!
//!  - `object` has a stronger focus on executable formats, while `symbolic` focusses on debugging
//!    information. This is why `symbolic` also includes a variant for PDBs and Breakpad objects,
//!    where `object` instead has a WASM variant.
//!  - `object` contains far more generic access to the data within objects at the cost of
//!    performance. `symbolic` tries to optimize for debugging scenarios at the cost of generic
//!    usage.
//!  - `symbolic` contains an abstraction for multi-object files ([`Archive`]), which is not easily
//!    possible in `object` due to the use of lifetimes on the `object::Object` trait.
//!
//! [`Object`]: enum.Object.html
//! [`Archive`]: enum.Archive.html
//! [`ObjectLike`]: trait.ObjectLike.html
//! [`DebugSession`]: trait.DebugSession.html
//! [`object`]: https://docs.rs/object

#![warn(missing_docs)]

mod base;
#[cfg(all(
    feature = "breakpad",
    feature = "dwarf",
    feature = "elf",
    feature = "macho",
    feature = "ms",
    feature = "sourcebundle",
    feature = "wasm"
))]
mod object;

#[cfg(feature = "breakpad")]
pub mod breakpad;
#[cfg(feature = "dwarf")]
pub mod dwarf;
#[cfg(feature = "elf")]
pub mod elf;
#[cfg(any(feature = "dwarf", feature = "breakpad"))]
pub mod function_builder;
#[cfg(feature = "ms")]
pub(crate) mod function_stack;
#[cfg(feature = "macho")]
pub mod macho;
#[cfg(feature = "ms")]
pub mod pdb;
#[cfg(feature = "ms")]
pub mod pe;
#[cfg(feature = "sourcebundle")]
pub mod sourcebundle;
#[cfg(feature = "wasm")]
pub mod wasm;

pub use crate::base::*;
#[cfg(all(
    feature = "breakpad",
    feature = "dwarf",
    feature = "elf",
    feature = "macho",
    feature = "ms",
    feature = "sourcebundle",
    feature = "wasm"
))]
pub use crate::object::*;
