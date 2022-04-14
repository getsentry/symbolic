//! Provides SymCache support.
//!
//! This includes a reader and writer for the binary format, as well as helper traits and functions
//! to apply transformations to debugging symbols before they are written to the SymCache.
//!
//! # Structure of a SymCache
//!
//! A SymCache (version 7) contains the following primary kinds of data, written in the following
//! order:
//!
//! 1. Files
//! 2. Functions
//! 3. Source Locations
//! 4. Address Ranges
//! 5. String Data
//!
//! The format uses `u32`s to represent line numbers, addresses, references, and string offsets.
//! Line numbers use `0` to represent an unknown or invalid value. Addresses, references, and string
//! offsets instead use `u32::MAX`.
//!
//! Strings are saved in one contiguous section with each individual string prefixed by 4 bytes
//! denoting its length. Functions and files refer to strings by an offset into this string section,
//! hence "string offset".
//!
//! ## Files
//!
//! A file contains string offsets for its file name, parent directory, and compilation directory.
//!
//! ## Functions
//!
//! A function contains string offsets for its name and compilation directory, a u32 for its entry
//! address, and a u32 representing the source language.
//!
//! ## Address Ranges
//!
//! Ranges are saved as a contiguous list of `u32`s, representing their starting addresses.
//!
//! ## Source Locations
//!
//! A source location in a symcache represents a possibly-inlined copy of a line in a source file.
//! It contains a line number, a reference to a file (see above), a reference to a function (ditto),
//! and a reference to the source location into which this source location was inlined. All of these
//! data are optional.
//!
//! ## Mapping From Ranges To Source Locations
//!
//! Every range in the SymCache is associated with at least one source location. As mentioned above,
//! each source location may in turn have a reference to a source location into which it is inlined.
//! Conceptually, each address range points to a sequence of source locations, representing a
//! hierarchy of inlined function calls.
//!
//! ### Example
//!
//! The mapping
//!
//! - `0x0001 - 0x002f`
//!   - `trigger_crash` in file `b.c`, line 12
//!   - inlined into `main` in file `a.c`, line 10
//! - `0x002f - 0x004a`
//!   - `trigger_crash` in file `b.c`, line 13
//!   - inlined into `main` in file `a.c`, line 10
//!
//! is represented like this in the SymCache (function/file name strings inlined for simplicity):
//! ```text
//! ranges: [
//!     0x0001 -> 1
//!     0x002f -> 2
//! ]
//!
//! source_locations: [{
//!     file: "a.c"
//!     line: 10
//!     function: "main"
//!     inlined_into: u32::MAX (not inlined)
//! }, {
//!     file: "b.c"
//!     line: 12
//!     function: "trigger_crash"
//!     inlined_into: 0 <- index reference to "main"
//! }, {
//!     file: "b.c"
//!     line: 13
//!     function: "trigger_crash"
//!     inlined_into: 0 <- index reference to "main"
//! }]
//! ```
//!
//! # Lookups
//!
//! To look up an address `addr` in a SymCache:
//!
//! 1. Find the range covering `addr` via binary search.
//! 2. Find the source location belonging to this range.
//! 3. Return an iterator over a series of source locations that starts at the source location found
//!    in step 2. The iterator climbs up through the inlining hierarchy, ending at the root source
//!    location.
//!
//! The returned source locations contain accessor methods for their function, file, and line
//! number.

#![warn(missing_docs)]

mod compat;
mod new;
mod old;
pub(crate) mod preamble;

pub use compat::*;
pub use new::transform;
pub use new::SymCacheWriter;
#[allow(deprecated)]
pub use old::format;
pub use old::{Line, LineInfo, SymCacheError, SymCacheErrorKind, ValueKind};

/// The latest version of the file format.
pub const SYMCACHE_VERSION: u32 = 7;

// Version history:
//
// 1: Initial implementation
// 2: PR #58:  Migrate from UUID to Debug ID
// 3: PR #148: Consider all PT_LOAD segments in ELF
// 4: PR #155: Functions with more than 65k line records
// 5: PR #221: Invalid inlinee nesting leading to wrong stack traces
// 6: PR #319: Correct line offsets and spacer line records
// 7: PR #459: A new binary format fundamentally based on addr ranges
