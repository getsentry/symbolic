//! Provides support for reading Portable PDB files,
//! specifically line information resolution for functions.
//!
//! [Portable PDB](https://github.com/dotnet/runtime/blob/main/docs/design/specs/PortablePdb-Metadata.md)
//! is a debugging information file format for Common Language Infrastructure (CLI) languages.
//! It is an extension of the [ECMA-335 format](https://www.ecma-international.org/wp-content/uploads/ECMA-335_6th_edition_june_2012.pdf).
//!
//! # Functionality
//!
//! * Parse Portable PDB files with [`PortablePdb::parse`].
//! * Convert Portable PDB files to [`PortablePdbCaches`](PortablePdbCache) with
//!   [`PortablePdbCacheConverter::process_portable_pdb`].
//! * Serialize `PortablePdbCaches` with [`PortablePdbCacheConverter::serialize`]
//!   and parse them with [`PortablePdbCache::parse`].
//! * Look up line information for a function on a `PortablePdbCache` with
//!   [`PortablePdbCache::lookup`].
//! ## Example
//! ```
//! use symbolic_ppdb::{LineInfo, PortablePdb, PortablePdbCacheConverter, PortablePdbCache};
//! let buf = std::fs::read("tests/fixtures/integration.pdb").unwrap();
//!
//! let pdb = PortablePdb::parse(&buf).unwrap();
//!
//! let mut converter = PortablePdbCacheConverter::new();
//! converter.process_portable_pdb(&pdb).unwrap();
//! let mut buf = Vec::new();
//! converter.serialize(&mut buf).unwrap();
//!
//! let cache = PortablePdbCache::parse(&buf).unwrap();
//! let line_info = cache.lookup(6, 10).unwrap();
//! assert_eq!(line_info.line, 55);
//! ```
//!
//! # Structure of a Portable PDB file
//! An ECMA-335 file is divided into sections called _streams_. The possible streams are
//! * `#~` ("metadata"), comprising information about classes, methods, modules, &c.,
//!   organized into tables adhering to various schemas. The original ECMA-335 tables
//!   are described in Section II.22 of the ECMA-335 spec, the tables added by Portable PDB are described
//!   in the Portable PDB spec.
//!   The [`MethodDebugInformation`](https://github.com/dotnet/runtime/blob/main/docs/design/specs/PortablePdb-Metadata.md#methoddebuginformation-table-0x31)
//!   table is of particular interest to `symbolic`, as it contains
//!   line information for functions.
//! * `#Strings`, comprising null-terminated UTF-8 strings.
//! * `#GUID`, a list of GUIDs.
//! * `#US` ("user strings"), comprising UTF-16 encoded strings.
//! * `#Blob`, comprising blobs of data that don't fit in any of the other streams.
//!
//! The Portable PDB format extends ECMA-335 by the addition of another steam, `#PDB`, as well
//! as several tables to the `#~` stream.

#![warn(missing_docs)]

mod cache;
mod format;

pub use cache::lookup::LineInfo;
pub use cache::writer::PortablePdbCacheConverter;
pub use cache::{CacheError, CacheErrorKind, PortablePdbCache};
pub use format::{FormatError, FormatErrorKind, PortablePdb};
