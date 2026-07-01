//! WebAssembly bindings for `symbolic`, published to npm as `@sentry/symbolic`.
//!
//! Exposes symbolic's functionality such as parsing of debug information files
//! (Mach-O/dSYM, ELF, PE/PDB, Portable PDB,  WebAssembly, Breakpad, SourceBundle).

pub mod debuginfo;
pub mod il2cpp;
mod utils;
