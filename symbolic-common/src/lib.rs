//! Common functionality for `symbolic`.
//!
//! This crate exposes a set of key types:
//!
//!  - [`ByteView`]: Gives access to binary data in-memory or on the file system.
//!  - [`SelfCell`]: Allows to create self-referential types.
//!  - [`Name`]: A symbol name that can be demangled with the `demangle` feature.
//!  - [`InstructionInfo`]: A utility type for instruction pointer heuristics.
//!  - Functions and utilities to deal with paths from different platforms.
//!
//! # Features
//!
//! - `serde` (optional): Implements `serde::Deserialize` and `serde::Serialize` for all data types.
//!   In the `symbolic` crate, this feature is exposed via `common-serde`.
//!
//! This module is part of the `symbolic` crate.
//!
//! [`Name`]: struct.Name.html
//! [`ByteView`]: struct.ByteView.html
//! [`InstructionInfo`]: struct.InstructionInfo.html
//! [`SelfCell`]: struct.SelfCell.html

#![warn(missing_docs)]

mod byteview;
mod cell;
mod heuristics;
mod path;
mod types;

pub use crate::byteview::*;
pub use crate::cell::*;
pub use crate::heuristics::*;
pub use crate::path::*;
pub use crate::types::*;

pub use debugid::*;
pub use uuid::Uuid;
