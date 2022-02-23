//! [Symbolic](https://docs.rs/symbolic) is a library written in Rust which is used at
//! [Sentry](https://sentry.io/) to implement symbolication of native stack traces, sourcemap
//! handling for minified JavaScript and more. It consists of multiple largely independent crates
//! which are bundled together into a C and Python library so it can be used independently of Rust.
//!
//! # What's in the package
//!
//! Symbolic provides the following functionality:
//!
//! - Symbolication based on custom cache files (symcache)
//! - Symbol cache file generators from:
//!   - Mach, ELF and PE symbol tables
//!   - Mach and ELF embedded DWARF data
//!   - PDB CodeView debug information
//!   - Breakpad symbol files
//! - Demangling support
//!   - C++ (GCC, clang and MSVC)
//!   - Objective C / Objective C++
//!   - Rust
//!   - Swift
//! - JavaScript sourcemap expansion
//!   - Basic token mapping
//!   - Heuristics to find original function names based on minified sources
//!   - Indexed sourcemap to sourcemap merging
//! - Proguard function mappings (via `symbolic-cabi` only, use `proguard` crate instead)
//! - Minidump / Breakpad processing
//!   - Generate Breakpad symbol files from Mach, ELF and PDBs
//!   - Process Minidumps to retrieve stack traces
//! - Convenient C and Python library
//! - Processing of Unreal Engine 4 native crash reports
//!   - Extract and process minidumps
//!   - Expose logs and UE4 context information
//!
//! # Usage
//!
//! Add `symbolic` as a dependency to your `Cargo.toml`. You will most likely want to activate some
//! of the features:
//!
//! - **`debuginfo`** (default): Contains support for various object file formats and debugging
//!   information. Currently, this comprises MachO and ELF (with DWARF debugging), PE and PDB, as
//!   well as Breakpad symbols.
//! - **`demangle`**: Demangling for Rust, C++, Swift and Objective C symbols. This feature requires
//!   a C++14 compiler on the PATH.
//! - **`minidump`**: Rust bindings for the Breakpad Minidump processor. Additionally, this includes
//!   facilities to extract stack unwinding information (sometimes called CFI) from object files.
//!   This feature requires a C++11 compiler on the PATH.
//! - **`sourcemap`**: Processing and expansion of JavaScript source maps, as well as lookups for
//!   minified function names.
//! - **`symcache`**: An optimized, platform-independent storage for common debugging information.
//!   This allows blazing fast symbolication of instruction addresses to function names and file
//!   locations.
//! - **`unreal`**: Processing of Unreal Engine 4 crash reports.
//!
//! There are also alternate versions for some of the above features that additionally add
//! implementations for `serde::{Deserialize, Serialize}` on suitable types:
//!
//! - **`common-serde`**
//! - **`debuginfo-serde`**
//! - **`minidump-serde`**
//! - **`unreal-serde`**
//!
//! ## Minimal Rust Version
//!
//! This crate is known to require at least Rust 1.41.

#![warn(missing_docs)]

#[doc(inline)]
pub use symbolic_common as common;
#[doc(inline)]
#[cfg(feature = "debuginfo")]
pub use symbolic_debuginfo as debuginfo;
#[doc(inline)]
#[cfg(feature = "demangle")]
pub use symbolic_demangle as demangle;
#[doc(inline)]
#[cfg(feature = "il2cpp")]
pub use symbolic_il2cpp as il2cpp;
#[doc(inline)]
#[cfg(feature = "minidump")]
pub use symbolic_minidump as minidump;
#[doc(inline)]
#[cfg(feature = "sourcemap")]
pub use symbolic_sourcemap as sourcemap;
#[doc(inline)]
#[cfg(feature = "symcache")]
pub use symbolic_symcache as symcache;
#[doc(inline)]
#[cfg(feature = "unreal")]
pub use symbolic_unreal as unreal;
