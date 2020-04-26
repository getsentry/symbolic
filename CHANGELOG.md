# Changelog

## 7.3.0

**Build Changes**:

To build with the `demangle` feature, a C++14 compiler is now required.

**Features**:

- Support Swift 5.2 mangling (#208)
- Inline docs from sub-crates (#209)
- Add Path utilities for dSYM structures (#212)
- Updated C++ demangler (#215)

**Bug Fixes**:

- Do not error in functions iterator on name errors (#201)
- Avoid infinite recursion in DWARF because of self references (#202)
- Do not skip symbols from SHT_NOBIT sections (#207)
- Do not assume sorted DWARF compilation units (#214)
- Skip eliminated functions in linked objects (#216)
- Avoid panics for UTF-8 characters in paths (#217)
- Get CFI info from eh_frame even if err in debug_frame (#218)
- Avoid `TooManyRegisterRules` errors in CFI (#219)
- Calculate correct line record sizes for DWARF (#220)
- Detect all scopes to fix incorrect inlinee hierarchies (#221)
- Patch all parent line records of inlinees in DWARF (#223)
- Fix broken compilation with -Clink-dead-code (#225)
- Return the same instruction address for inlinees in symcaches (#226)

## 7.2.0

**Features**:

- Upgrade UUID-related dependencies (#199)

## 7.1.1

**Features**:

- Implement `serde::{Deserialize, Serialize}` for `ProcessResult` (#188)
- Implement `serde::{Deserialize, Serialize}` for `Name` (#191)
- Update the `gimli`, `goblin` and `pdb` libraries (#196)

**Bug Fixes**:

- Do not skip DWARF units with a `DW_AT_low_pc` of `0` (#173)
- Search for MachO sections in all segments (#173)
- Fix processing Hermes source maps with non-hermes stack frames (#189)
- Fix decompression of GNU compressed debug sections (`.zdebug_info`) (#192)

## 7.1.0

_This release is not available on crates.io_

**Features**:

- Support skipping over files when creating source bundles (#167)
- Support for React Native Hermes source maps (#187)

**Bug Fixes**:

- Resolved an error in processing DWARF CFI
- Resolved an error reading ELF fiels with stripped `PT_DYNAMIC` header
- Support for Breakpad functions without names
- Multiple fixes in PDB and PE file processing
- Fix compilation with MSVC (#164)
- Added unmapped MachO object types (#169)
- Proper detection for ELF stripped debug companion files (#170)
- Detect Java class files which share the same magic as MachO files (#172)
- Fix memory leaks in the python binding (#180)

## 7.0.0

_This release is not available on crates.io_

**New Features**:

- A new API to parse Unreal Engine 4 Crash reports (#152).
- Source bundles to resolve source code for stack frames (#154).
- Inline functions for Microsoft PDBs (#160).
- Improved demangling of C++ symbols.

**Bug Fixes**:

- Resolved unexpected EOF when parsing certain PDBs.
- Restored compatibility with Python 3 (#158).

## 6.1.4

**Common**:

- Add `ARM64_32` (ILP32 ABI on 64-bit ARM) (#149).
- Support architecture names from apple crash reports (#151).

**DebugInfo**:

- Fix invalid memory addresses for some ELF files (#148).
- Prefer a PDB's age from the DBI stream (#150).
- Do not emit default CFI for the `.ra` register (#157).

**Minidump**:

- Fix a memory leak when processing minidumps (#146).

**SymCache**:

- Add `is_latest()` to symcaches and CFI caches.
- Support functions with more than 65k line records (#155).

## 6.1.3

**Common**:

- Support MIPS and MIPS64 (#141).

**DebugInfo**:

- Fix code identifiers for PE files and do not return empty ones (#139, #142).
- Support Breakpad debug identifiers without an age field (#140).
- Add `Archive::is_multi` to check for multi-architecture archives (#143).

**Minidump**:

- Add more trait implementations to minidump processor types.
- Process minidumps without thread lists (#144).
- Update the breakpad processor. This allows to stackwalk Unreal Engine 4 minidumps (#145).

## 6.1.2

- Demangling support for Swift 5.
- Fix a performance regression in 6.1.1

## 6.1.1

- Expose PDB file names from PE object files.
- Fix incorrect CFI extraction from ELF files.
- Fix broken symcache lookups for certain optimized files.

## 6.1.0

- Support PDB file and line information.
- Support stack unwind info in PDB files (32-bit).
- Support stack unwind info in PE files (64-bit).
- Fix breakpad CFI generation for functions pushing machine frames.

## 6.0.6

- Add `normalize_code_id` in the Python package and C layer.
- Add `ByteView::map_file` to create a memory map directly from a file handle.
- Add size attribute to streams returned from Minidumps / UE4 crash reports.

## 6.0.5

- Normalize code identifiers to lowercase (#133).

## 6.0.4

- Exposes code identifiers and debug file names for minidumps in Python. Previously, this was only
  available in the Rust Crate.
- `ObjectLookup` now supports `code_file` and `debug_id` in in Python.

## 6.0.3

Re-release on crates.io.

## 6.0.2

**This release is broken on crates.io**

- Fix Rust features: The `serde` feature activated minidump and unreal unintentionally. This is
  addressed by providing separate features for modules with serde. See the Readme for more information.
- Include breakpad sources in `symbolic-minidump`.

## 6.0.0

This is a complete rewrite of `symbolic`. The aim of this release is to make the Rust version, the
C-API and the Python package more convenient to use in different scenarios. As a result, there have
been quite a few breaking changes.

**Breaking Changes:**

- `ByteViewHandle` has been replaced with the slightly safer type `SelfCell`. It allows to create a
  self-referential pair of an owning object and a derived object.
- `Archive` and `Object` are the new types to interface with debug information. There are also
  direct types exposed for Breakpad, ELF, MachO, PE and PDB; as well as traits to abstract over
  them.
- `SymCache` has a cleaner API, and the writing part has been moved to `SymCacheWriter`.
- Some common types have received better names: `ObjectKind` is now called `FileFormat`.
  `ObjectClass` is now called `ObjectKind`.
- Many more small signature changes, such as zero-copy return values or iterators instead of
  collections.

**New Features:**

- Initial support for PE and PDB is here. It is not complete yet, and will be expanded over the next
  releases.
- Symbol tables for ELF are now supported. On the bottom line, this will improve symbolication
  results on Linux.
- GNU-style compressed debug information (e.g. `.zdebug_info`) is now supported.
- Support for most of DWARF 5, thanks to the amazing work on the `gimli` crate.
- More lenient parsing of Breakpad symbols now handles certain edge cases more gracefully and gives
  much better error messages.
- More utilities to join or split paths from any platform.

**Bug Fixes:**

- Fix invalid function name resolution for certain DWARF files.
- Fix errors on DWARF files generated with LTO.
- Fix memory leaks when processing Minidumps from Python.
- Skip STAB symbol entries in MachO files, potentially leading to wrong function names.
- Do not error on "negative" line numbers in Breakpad symbols.

**Internal Changes:**

- Greatly simplified build process and better documentation for the C library.
- Improved test suite, docs and READMEs. While there can never be enough tests, this is a
  significant step to improving the overall quality of symbolic.
- Automatic cloning of submodules during the build. This should make it easier to start developing
  on `symbolic`.
