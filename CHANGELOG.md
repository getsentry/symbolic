# Changelog

## 9.0.0

**Breaking changes**:

- Updated the `debugid` dependency to 0.8.
- Updated the `uuid` dependency to 1.0.
- Updated the `pdb` dependency to 0.8.
- Removed the public method `symbolic_common::CpuFamily::cfi_register_name`.
- The `symbolic-minidump` crate has been dropped. The CFI functionality that was contained in
  `symbolic-minidump` now resides in its own crate, `symbolic-cfi`.
- The `symbolic-unwind` crate has been dropped.
- The `symbolic-sourcemap` crate has been dropped. Since it was only used in `symbolic-cabi`, its
  functionality has been incorporated into `symbolic-cabi`.
- Support for symcache versions before v7 has been dropped. This entails a number of changes in
  the public API of `symbolic-symcache`:
  - Removed support for symcache binary formats prior to v7.
  - Removed `SymCacheWriter`.
  - Removed `SymCacheError`.
  - Removed `SymCacheErrorKind`.
  - Removed `Line`.
  - Removed `Lines`.
  - Removed `LineInfo`.
  - Removed `Lookup`.
  - Removed `Function::id`.
  - Removed `Function::parent_id`.
  - Removed `Function::address`.
  - Removed `Function::symbol`.
  - Removed `Function::compilation_dir`.
  - Removed `Function::lines`.
  - Removed `SymCache::has_line_info`.
  - Removed `SymCache::has_file_info`.
  - Changed return type of `Function::name` to string slice.
  - Changed return type of `SymCache::lookup` to `SourceLocations`.
  - Added `Function::name_for_demangling` with the previous signature and behavior of `Function::name`.
  - Added `Function::entry_pc`.
  - Added `SymCacheConverter`.
  - Added `Error`.
  - Added `ErrorKind`.
  - Added `File`.
  - Added `Files`.
  - Added `FilesDebug`.
  - Added `FunctionsDebug`.
  - Added `SourceLocation`.
  - Added `SourceLocations`.
  - Added `SymCache::files`.
  - Added lifetime parameter to `Transformers`.
  - Undeprecated `Function` and `Functions`.
  - Undeprecated `SymCache::functions`.
- Some C and Python bindings have been dropped or adjusted. Concretely:
  - `symbolic-cabi::minidump` and the corresponding Python functionality has been removed. The
    CFI functionality that was contained therein now resides in `symbolic-cabi::cfi` and `symbolic.cfi`,
    respectively.
  - `symbolic-cabi::unreal` and the corresponding Python functionality has been removed.
  - `symbolic-cabi::symcache::SymbolicLineInfo` has been replaced with `SymbolicSourceLocation`,
    which has a different interface. Likewise, `symbolic.symcache.LineInfo` has been replaced with
    `SourceLocation`.
  - `symbolic-cabi::symcache::symbolic_symcache_has_file_info` and `symbolic_symcache_has_line_info`
    have been removed, likewise for `symbolic.symcache.SymCache.has_line_info` and `has_file_info`.

## 8.8.0

**Features**:

- Optionally collect referenced C# file sources when creating a source bundle. ([#516](https://github.com/getsentry/symbolic/pull/516))

**Fixes**:

- Only skip one function when encountering unknown Unwind Codes on Windows x64. ([#588](https://github.com/getsentry/symbolic/pull/588))
- Skip over low_pc sentinels instead of erroring. ([#590](https://github.com/getsentry/symbolic/pull/590))

## 8.7.3

**Fixes**:

- Make CFI generation for Windows x64 more accurate, restoring all possible registers and supporting frame pointer unwinding. ([#549](https://github.com/getsentry/symbolic/pull/549))

## 8.7.2

**Fixes**:

- Make sure to correctly parse Unreal crash reports with zero-length files ([#565](https://github.com/getsentry/symbolic/pull/565))

## 8.7.1

**Fixes**:

- Updated wasmparser dependency to `0.83.0` ([#557](https://github.com/getsentry/symbolic/pull/557))
- Updated rust-sourcemap dependency to hopefully speed up sourcemap parsing ([#559](https://github.com/getsentry/symbolic/pull/559))
- Match symbol names by exact addresses from symbol table ([#510](https://github.com/getsentry/symbolic/pull/510))
- Return a more correct `function_size` when dealing with split functions ([#522](https://github.com/getsentry/symbolic/pull/522))

**Thank you**:

Features, fixes and improvements in this release have been contributed by:

- [@shuoli84](https://github.com/shuoli84)
- [@bnjbvr](https://github.com/bnjbvr)

## 8.7.0

**Features**:

- Added a new SymCache `Transformer`, which can be used to apply Function or SourceLocation transformations. ([#496](https://github.com/getsentry/symbolic/pull/496))
- Turn the breakpad-based minidump processor into an optional feature flag. ([#519](https://github.com/getsentry/symbolic/pull/519))

**Fixes**:

- Fixed CFI `STACK WIN` records being written correctly. ([#513](https://github.com/getsentry/symbolic/pull/513))
- Do not consider empty files as valid BcSymbolMaps anymore. ([#523](https://github.com/getsentry/symbolic/pull/523))
- Fix wasm parsing rejecting valid wasm files with non-default features. ([#520](https://github.com/getsentry/symbolic/pull/520))

**Thank you**:

Features, fixes and improvements in this release have been contributed by:

- [@bnjbvr](https://github.com/bnjbvr)

## 8.6.1

**Fixes**:

- Update `goblin` which received fixes to avoid panics and unreasonable memory allocations based on invalid input. ([#503](https://github.com/getsentry/symbolic/pull/503))
- Fix wrong instruction addresses of the first frame in ARM and ARM64 minidumps. The addresses were incorrectly incremented by one instruction size. ([#504](https://github.com/getsentry/symbolic/pull/504))
- Correctly skip ELF sections with an offset of `0` instead of ignoring all following sections. This bug may have lead to missing unwind or debug information. ([#505](https://github.com/getsentry/symbolic/pull/505))
- Detect unwind information when linking with `gold`. ([#505](https://github.com/getsentry/symbolic/pull/505))

## 8.6.0

**Features**:

- Added a new SymCache binary format which is fundamentally based around instruction addr ranges.
- Add `ElfObject::debug_link` that allows recovering the [debug link](https://sourceware.org/gdb/onlinedocs/gdb/Separate-Debug-Files.html) from an Elf if present. ([#450](https://github.com/getsentry/symbolic/pull/450))
- Updated Swift demangler to 5.5.1. ([#465](https://github.com/getsentry/symbolic/pull/465))
- Support split functions. ([#441](https://github.com/getsentry/symbolic/pull/441))
- Refactor `symbolic-debuginfo` feature flags. ([#470](https://github.com/getsentry/symbolic/pull/470))
- Rewrite wasm parser. ([#474](https://github.com/getsentry/symbolic/pull/474))

**Fixes**:

- Make SourceBundle ordering deterministic. ([#489](https://github.com/getsentry/symbolic/pull/489))
- Replace unmaintained dependencies.
- Better guard against invalid input that could lead to unreasonable memory allocations, panics or infinite loops.

**Thank you**:

Features, fixes and improvements in this release have been contributed by:

- [@dureuill](https://github.com/dureuill)
- [@Jake-Shadle](https://github.com/Jake-Shadle)

## 8.5.0

**Features**:

- Add `ByteView::map_file_ref` constructor which does not consume the `File` passed to it. ([#448](https://github.com/getsentry/symbolic/pull/448))

**Fixes**:

- Support Unreal Engine 5 crash reporter. ([#449](https://github.com/getsentry/symbolic/pull/449))

## 8.4.0

**Features**:

- Add `Unreal4Crash::parse_with_limit` which allows specifying a maximum allocation size when extracting compressed UE4 crash archives. ([#447](https://github.com/getsentry/symbolic/pull/447))

**Fixes**:

- Apply speculative handling of stackless functions only on `amd64` when creating CFI caches. ([#445](https://github.com/getsentry/symbolic/pull/445))

## 8.3.2

**Features**:

- Build and publish binary wheels for `arm64` / `aarch64` on macOS and Linux. ([#442](https://github.com/getsentry/symbolic/pull/442))

**Fixes**:

- Don’t prefix ARM registers with `$` for CFI files. ([#443](https://github.com/getsentry/symbolic/pull/443))

**Thank you**:

Features, fixes and improvements in this release have been contributed by:

- [@Gankra](https://github.com/Gankra)

## 8.3.1

**Fixes**:

- Avoid panic when looking for hex suffixes in multibyte character strings in the demangler. ([#430](https://github.com/getsentry/symbolic/pull/430))
- Allow processing of ELF files as long as they have valid program and section headers. ([#434](https://github.com/getsentry/symbolic/pull/434))
- Expose dynamic symbols in ELF files. ([#421](https://github.com/getsentry/symbolic/pull/421))
- Make dsym_parent accept `.framework.dSYM`. ([#425](https://github.com/getsentry/symbolic/pull/425))

**Thank you**:

Features, fixes and improvements in this release have been contributed by:

- [@goffrie](https://github.com/goffrie)
- [@gabrielesvelto](https://github.com/gabrielesvelto)
- [@luser](https://github.com/luser)
- [@mstange](https://github.com/mstange)

## 8.3.0

**Features**:

- Write versioned CFI Cache files. Reading those files is only supported with symbolic versions `>= 8.2.1`, so trying to use a CFI Cache file with an older version of symbolic will fail with a `CfiErrorKind::BadFileMagic` error.

**Fixes**:

- Correctly restore callee saves registers when using compact unwind info.
- Correctly map all DWARF information when using BcSymbolMaps.
- Allow processing of PDB files that have broken inlinee file references.
- Skip duplicated DWARF functions which can lead to `inline parent offset` overflows.

## 8.2.1

**Features**:

- Add support for reading versioned CFI Cache files.

**Fixes**:

- Avoid quadratic slowdown when using compact unwind info on macOS.

**Thank you**:

Features, fixes and improvements in this release have been contributed by:

- [@Gankra](https://github.com/Gankra)

## 8.2.0

**Caution**:

- Relevant dependencies such as `gimli`, `goblim`, and `wasm`-related libraries were updated.

**Features**:

- Support for compact unwind info in MachO files was added, along with special casing of some well known macOS system functions.
- The parser of the Breakpad Format was rewritten.

**Bug Fixes**:

- All valid `STACK WIN` record types are being parsed correctly. This did add new variants to the `BreakpadStackWinRecordType` enum. Technically a _breaking change_, but we do not consider the Breakpad Parser types as adhering to strict SemVer rules.

**Thank you**:

Features, fixes and improvements in this release have been contributed by:

- [@Gankra](https://github.com/Gankra)

## 8.1.0

**Features**:

- Add support for loading BCSymbolMaps into MachObjects to un-obfuscate symbol names in bitcode builds. ([#336](https://github.com/getsentry/symbolic/pull/336))

**Bug Fixes**:

- Handle too many files more gracefully. ([#374](https://github.com/getsentry/symbolic/pull/374))
- Parse .pdb files containing modules without symbols. ([pdb#102](https://github.com/willglynn/pdb/pull/102))

## 8.0.5

**Bug Fixes**:

- Fix detecting hidden Swift symbols in `MachObject::requires_symbolmap`. Additionally, the MachO symbol iterator no longer strips underscores from `__hidden#` symbols. ([#316](https://github.com/getsentry/symbolic/pull/316))

## 8.0.4

Manylinux2010 has dropped support for Python 2.7. As a result, we're no longer building or testing the Python package with Python 2.7. This and future releases require at least Python 3.6.

**Bug Fixes**:

- Compute correct line offsets in symcaches with large gaps between line records. ([#319](https://github.com/getsentry/symbolic/pull/319))
- Support symcache lookups for public symbols larger than 65k. ([#320](https://github.com/getsentry/symbolic/pull/320))
- Fixed bug that caused functions to have a length of `0` in symcaches. ([#324](https://github.com/getsentry/symbolic/pull/324))
- Support `debug_addr` indexes in DWARF functions. ([#326](https://github.com/getsentry/symbolic/pull/326))

## 8.0.3

**Bug Fixes**:

- Support DWARF information from MIPS compilers in `SHT_MIPS_DWARF` sections. ([#317](https://github.com/getsentry/symbolic/pull/317))
- Remove a duplicate dependency to two versions of `walrus` for WASM parsing. ([#312](https://github.com/getsentry/symbolic/pull/312))

## 8.0.2

**Bug Fixes**:

- Include third-party submodules to allow the Python `sdist` to build again. ([#310](https://github.com/getsentry/symbolic/pull/310))

## 8.0.1

**Bug Fixes**:

- Compute correct debug identifiers when proessing a Minidump from a machine with opposite endianness. This particularly allows to process MIPS minidumps on little-endian hosts. ([#281](https://github.com/getsentry/symbolic/pull/281))
- Update the breakpad processor with better stack scanning heuristics. Some false-positive frames are avoided during stack scanning now. ([#281](https://github.com/getsentry/symbolic/pull/281))
- Avoid panics when processing UE4 crash logs containing ambiguous local times. ([#307](https://github.com/getsentry/symbolic/pull/307))

## 8.0.0

**Breaking Changes**:

- Usage of `failure` was removed, and all error types were changed to only implement `std::error::Error` and related traits.
- `symbolic-proguard` was removed in favor of the `proguard` crate. Proguard is still supported via `symbolic-cabi` and the python API however.
- Deprecated APIs have been removed:
  - `InstructionInfo`'s fields are no longer public.
  - `pointer_size`, `instruction_alignment` and `ip_register_name` have moved from `Arch` to `CpuFamily`.
  - `Arch::register_name` as been moved to `CpuFamily::cfi_register_name`.
  - `Dwarf::raw_data` and `Dwarf::section_data` have been replaced with the `raw_section` and `section` APIs.
  - `Unreal4ContextRuntimeProperties::misc_primary_cpu_brand` is has been removed.
- Deprecated Python APIs have been removed:
  - `CodeModule.id` and `CodeModule.name` Use `debug_id` and `code_file`, respectively.
- `DemangleFormat` and public fields of `DemangleOptions` have been removed in favor of builder methods on `DemangleOptions`.
- `Name::new` now takes both the `NameMangling` state, and the `Language` explicitly.

**Features**:

- Add support for the `wasm32` architecture. ([#166](https://github.com/getsentry/symbolic/pull/166))
- Support demangling for Swift 5.3. ([#282](https://github.com/getsentry/symbolic/pull/282))

**Bug Fixes**:

- Detect mangled anonymous namespaces in PDB inlinees ([#261](https://github.com/getsentry/symbolic/pull/261))
- Fix a panic due to undefined behavior. ([#287](https://github.com/getsentry/symbolic/pull/287))
- Skip line program sequences at 0. ([#291](https://github.com/getsentry/symbolic/pull/291))
- Prefer DWARF names for Dart functions. ([#293](https://github.com/getsentry/symbolic/pull/293))

## 7.5.0

**Changes**:

- Add missing unreal data attributes (`EngineData` and `GameData`). ([#257](https://github.com/getsentry/symbolic/pull/257))
- Expose binary names for ELF and MachO ([#252](https://github.com/getsentry/symbolic/pull/252))
- Mark enums as `non_exhaustive`. ([#256](https://github.com/getsentry/symbolic/pull/256))
- Add method to create Archive from bytes. ([#250](https://github.com/getsentry/symbolic/pull/250))

**Bug Fixes**:

- Fix compilation errors on nightly Rust due to a lifetime mismatch. This is temporarily solved with a statically verified unsafe transmute, which will be replaced in an upcoming breaking change. ([#258](https://github.com/getsentry/symbolic/pull/258))

## 7.4.0

**Deprecations**:

- `pointer_size`, `instruction_alignment` and `ip_register_name` have moved from `Arch` to `CpuFamily`.
- `Arch::register_name` as been moved to `CpuFamily::cfi_register_name`.
- Field access on `InstructionInfo` has been deprecated and replaced
  with a builder.

**Changes**:

- More detailed documentation and examples on all types and functions in `symbolic-common`. ([#246](https://github.com/getsentry/symbolic/pull/247))

**Bug Fixes**:

- `CpuFamily::cfi_register_name` returns `None` instead of `Some("")` for some unknown registers.
- Update `cpp_demangle` again after the previous release was yanked. ([#247](https://github.com/getsentry/symbolic/pull/247))

## 7.3.6

**Bug Fixes**:

- Update the `cpp_demangle` dependency to fix broken builds after a breaking change. ([#244](https://github.com/getsentry/symbolic/pull/244), thanks @o0Ignition0o)

## 7.3.5

**Bug Fixes**:

- Update the `proguard` dependency to fix line info detection. ([#242](https://github.com/getsentry/symbolic/pull/242))

## 7.3.4

**Deprecations**:

- `symbolic-proguard` is now deprecated and will be removed in the next major release. Use the `proguard` crate directly. The C-bindings and Python interface will remain. ([#240](https://github.com/getsentry/symbolic/pull/240))

**Python**:

- Switch the C-ABI and python to `proguard 4.0.0` which supports frame remapping. ([#240](https://github.com/getsentry/symbolic/pull/240))

**Bug Fixes**:

- Fix broken links in docs on `ByteView`, `SelfCell` and `AsSelf`. ([#241](https://github.com/getsentry/symbolic/pull/241))

## 7.3.3

**Bug Fixes**:

- Update broken doc comments for `SelfCell` and `debuginfo::dwarf` ([#238](https://github.com/getsentry/symbolic/pull/238))
- Fix holes in line records of inline parents in DWARF ([#239](https://github.com/getsentry/symbolic/pull/239))

## 7.3.2

**Bug Fixes**:

- Fix line information of inline parents in DWARF ([#237](https://github.com/getsentry/symbolic/pull/237)). Many thanks to @calixteman!

## 7.3.1

**Bug Fixes**:

- Skip invalid PE runtime function entries ([#230](https://github.com/getsentry/symbolic/pull/230))
- Support demangling of block invocation functions ([#229](https://github.com/getsentry/symbolic/pull/229))
- Skip invalid CFI entries instead of erroring out ([#232](https://github.com/getsentry/symbolic/pull/232))
- Detect stub DLLs and skip CFI generation ([#233](https://github.com/getsentry/symbolic/pull/233))
- Skip functions with unknown unwind codes ([#234](https://github.com/getsentry/symbolic/pull/234))
- Update `goblin` to fix panics in PE unwinding ([#231](https://github.com/getsentry/symbolic/pull/231))
- Update `gimli` to to support `eh_frame` CIE version 3 ([#231](https://github.com/getsentry/symbolic/pull/231))
- Update `cpp_demangle` ([#231](https://github.com/getsentry/symbolic/pull/231))

## 7.3.0

**Build Changes**:

To build with the `demangle` feature, a C++14 compiler is now required.

**Features**:

- Support Swift 5.2 mangling ([#208](https://github.com/getsentry/symbolic/pull/208))
- Inline docs from sub-crates ([#209](https://github.com/getsentry/symbolic/pull/209))
- Add Path utilities for dSYM structures ([#212](https://github.com/getsentry/symbolic/pull/212))
- Updated C++ demangler ([#215](https://github.com/getsentry/symbolic/pull/215))

**Bug Fixes**:

- Do not error in functions iterator on name errors ([#201](https://github.com/getsentry/symbolic/pull/201))
- Avoid infinite recursion in DWARF because of self references ([#202](https://github.com/getsentry/symbolic/pull/202))
- Do not skip symbols from SHT_NOBIT sections ([#207](https://github.com/getsentry/symbolic/pull/207))
- Do not assume sorted DWARF compilation units ([#214](https://github.com/getsentry/symbolic/pull/214))
- Skip eliminated functions in linked objects ([#216](https://github.com/getsentry/symbolic/pull/216))
- Avoid panics for UTF-8 characters in paths ([#217](https://github.com/getsentry/symbolic/pull/217))
- Get CFI info from eh_frame even if err in debug_frame ([#218](https://github.com/getsentry/symbolic/pull/218))
- Avoid `TooManyRegisterRules` errors in CFI ([#219](https://github.com/getsentry/symbolic/pull/219))
- Calculate correct line record sizes for DWARF ([#220](https://github.com/getsentry/symbolic/pull/220))
- Detect all scopes to fix incorrect inlinee hierarchies ([#221](https://github.com/getsentry/symbolic/pull/221))
- Patch all parent line records of inlinees in DWARF ([#223](https://github.com/getsentry/symbolic/pull/223))
- Fix broken compilation with -Clink-dead-code ([#225](https://github.com/getsentry/symbolic/pull/225))
- Return the same instruction address for inlinees in symcaches ([#226](https://github.com/getsentry/symbolic/pull/226))

## 7.2.0

**Features**:

- Upgrade UUID-related dependencies ([#199](https://github.com/getsentry/symbolic/pull/199))

## 7.1.1

**Features**:

- Implement `serde::{Deserialize, Serialize}` for `ProcessResult` ([#188](https://github.com/getsentry/symbolic/pull/188))
- Implement `serde::{Deserialize, Serialize}` for `Name` ([#191](https://github.com/getsentry/symbolic/pull/191))
- Update the `gimli`, `goblin` and `pdb` libraries ([#196](https://github.com/getsentry/symbolic/pull/196))

**Bug Fixes**:

- Do not skip DWARF units with a `DW_AT_low_pc` of `0` ([#173](https://github.com/getsentry/symbolic/pull/173))
- Search for MachO sections in all segments ([#173](https://github.com/getsentry/symbolic/pull/173))
- Fix processing Hermes source maps with non-hermes stack frames ([#189](https://github.com/getsentry/symbolic/pull/189))
- Fix decompression of GNU compressed debug sections (`.zdebug_info`) ([#192](https://github.com/getsentry/symbolic/pull/192))

## 7.1.0

_This release is not available on crates.io_

**Features**:

- Support skipping over files when creating source bundles ([#167](https://github.com/getsentry/symbolic/pull/167))
- Support for React Native Hermes source maps ([#187](https://github.com/getsentry/symbolic/pull/187))

**Bug Fixes**:

- Resolved an error in processing DWARF CFI
- Resolved an error reading ELF fiels with stripped `PT_DYNAMIC` header
- Support for Breakpad functions without names
- Multiple fixes in PDB and PE file processing
- Fix compilation with MSVC ([#164](https://github.com/getsentry/symbolic/pull/164))
- Added unmapped MachO object types ([#169](https://github.com/getsentry/symbolic/pull/169))
- Proper detection for ELF stripped debug companion files ([#170](https://github.com/getsentry/symbolic/pull/170))
- Detect Java class files which share the same magic as MachO files ([#172](https://github.com/getsentry/symbolic/pull/172))
- Fix memory leaks in the python binding ([#180](https://github.com/getsentry/symbolic/pull/180))

## 7.0.0

_This release is not available on crates.io_

**New Features**:

- A new API to parse Unreal Engine 4 Crash reports ([#152](https://github.com/getsentry/symbolic/pull/152)).
- Source bundles to resolve source code for stack frames ([#154](https://github.com/getsentry/symbolic/pull/154)).
- Inline functions for Microsoft PDBs ([#160](https://github.com/getsentry/symbolic/pull/160)).
- Improved demangling of C++ symbols.

**Bug Fixes**:

- Resolved unexpected EOF when parsing certain PDBs.
- Restored compatibility with Python 3 ([#158](https://github.com/getsentry/symbolic/pull/158)).

## 6.1.4

**Common**:

- Add `ARM64_32` (ILP32 ABI on 64-bit ARM) ([#149](https://github.com/getsentry/symbolic/pull/149)).
- Support architecture names from apple crash reports ([#151](https://github.com/getsentry/symbolic/pull/151)).

**DebugInfo**:

- Fix invalid memory addresses for some ELF files ([#148](https://github.com/getsentry/symbolic/pull/148)).
- Prefer a PDB's age from the DBI stream ([#150](https://github.com/getsentry/symbolic/pull/150)).
- Do not emit default CFI for the `.ra` register ([#157](https://github.com/getsentry/symbolic/pull/157)).

**Minidump**:

- Fix a memory leak when processing minidumps ([#146](https://github.com/getsentry/symbolic/pull/146)).

**SymCache**:

- Add `is_latest()` to symcaches and CFI caches.
- Support functions with more than 65k line records ([#155](https://github.com/getsentry/symbolic/pull/155)).

## 6.1.3

**Common**:

- Support MIPS and MIPS64 ([#141](https://github.com/getsentry/symbolic/pull/141)).

**DebugInfo**:

- Fix code identifiers for PE files and do not return empty ones ([#139](https://github.com/getsentry/symbolic/pull/139), #142).
- Support Breakpad debug identifiers without an age field ([#140](https://github.com/getsentry/symbolic/pull/140)).
- Add `Archive::is_multi` to check for multi-architecture archives ([#143](https://github.com/getsentry/symbolic/pull/143)).

**Minidump**:

- Add more trait implementations to minidump processor types.
- Process minidumps without thread lists ([#144](https://github.com/getsentry/symbolic/pull/144)).
- Update the breakpad processor. This allows to stackwalk Unreal Engine 4 minidumps ([#145](https://github.com/getsentry/symbolic/pull/145)).

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

- Normalize code identifiers to lowercase ([#133](https://github.com/getsentry/symbolic/pull/133)).

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
