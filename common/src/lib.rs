//! Common functionality for `symbolic`.
//!
//! This crate exposes a set of key types:
//!
//!  - [`ByteView`]: Gives access to binary data in-memory or on the file system.
//!  - [`SelfCell`]: Allows to create self-referential types.
//!  - [`Name`]: A symbol name that can be demangled with the `demangle` feature.
//!  - ... and some useful functions to deal with paths in different platforms.
//!
//! [`Name`]: struct.Name.html
//! [`ByteView`]: struct.ByteView.html
//! [`SelfCell`]: struct.SelfCell.html

#![warn(missing_docs)]

mod byteview;
mod cell;
mod fail;
mod heuristics;
mod path;
mod types;

pub use crate::byteview::*;
pub use crate::cell::*;
pub use crate::fail::*;
pub use crate::heuristics::*;
pub use crate::path::*;
pub use crate::types::*;

pub use debugid::*;
pub use uuid::Uuid;
