# Security Model

Last reviewed: draft · Applies to: this directory and below

## System and Scope

Symbolic is a Rust library for parsing debug information files, building and
reading symbolication caches, demangling symbol names, processing JavaScript
source maps, and reading Unreal Engine crash reports. It ships as Rust crates
(the symbolic-* crates), a C ABI (`symbolic-cabi`), a Python package (`py/`), and an
experimental WebAssembly package (`symbolic-wasm`, published to npm).

Symbolic has no network surface of its own. Its exposure comes from its
consumers, which pass it bytes that originate outside Sentry:

- Symbolicator parses customer-uploaded debug files, files fetched from
  customer-configured and public symbol servers, and symbol names from SDK
  events. It runs symbolic in-process for many organizations at once.
- Relay parses Unreal Engine crash reports received from the internet.
- Sentry uses the Python package to inspect uploaded debug files.
- sentry-cli and the npm package parse files on developer and CI machines.

Every symbolic-* crate and the vendored code compiled into
`symbolic-demangle` (`symbolic-demangle/vendor/`) is in scope.

## Threat Model and Trust Boundaries

Assets: the availability and memory integrity of the processes that embed
symbolic, which are shared across tenants in Symbolicator and Relay, and the
correctness of the data returned to them.

Actors and inputs:

- **Uploader of a debug or crash file (attacker-controlled).** Controls every
  byte of Mach-O, ELF, PE, PDB, Portable PDB, WASM, Breakpad, source bundle,
  IL2CPP line mapping, JavaScript, source map, and Unreal crash report input,
  including embedded compressed sections and sources, and every symbol name
  passed to the demangler.
- **Symbol server operator (attacker-controlled).** Equivalent to an uploader.
- **Cache store (semi-trusted).** SymCache, SourceMapCache, and
  PortablePdbCache files are written by the consumer from parsed input and read
  back later. Writing a malformed cache directly requires access to the cache
  store. Trust level: Unresolved (proposed semi-trusted).
- **Embedding application (trusted).** Chooses resource limits through
  `ParseObjectOptions` and `Unreal4Crash::parse_with_limit`, owns the buffers it
  passes in, and calls the C ABI and Python API as documented.
- **Operator running the source bundle writer (trusted).** Points
  `SourceBundleWriter::write_object` at their own build output.

The trust boundary is the byte buffer passed to any parse or demangle entry
point. Nothing inside that buffer is trusted, including declared sizes, counts,
offsets, and nesting depths.

## Component Map

Where the security-critical code lives, grouped by the trust level of its
input. Paths are relative to the repository root.

Untrusted input parsers (attacker-controlled bytes):

| Path | Purpose | Invariants |
| --- | --- | --- |
| `symbolic-debuginfo/src/object.rs` | Detects the file format and dispatches to a parser; defines `ParseObjectOptions`. | INV-3 |
| `symbolic-debuginfo/src/elf.rs` | ELF parsing, including zlib and zstd compressed debug sections. | INV-1, INV-3 |
| `symbolic-debuginfo/src/macho/` | Mach-O and fat archives, compact unwind info, BCSymbolMaps. | INV-1, INV-2 |
| `symbolic-debuginfo/src/pe.rs` | PE parsing, including deflate-compressed embedded Portable PDBs. | INV-1, INV-3 |
| `symbolic-debuginfo/src/pdb/` | Microsoft PDB parsing and the srcsrv stream. | INV-1, INV-2 |
| `symbolic-debuginfo/src/ppdb.rs` | Portable PDB object wrapper and embedded source extraction. | INV-3 |
| `symbolic-debuginfo/src/dwarf/` | DWARF debug info, inline function trees, and name resolution through abstract origins. | INV-2 |
| `symbolic-debuginfo/src/function_builder.rs` | Builds nested function and inlinee trees from parsed debug info. | INV-2, INV-3 |
| `symbolic-debuginfo/src/breakpad.rs` | Text parser for Breakpad symbol files. | INV-2 |
| `symbolic-debuginfo/src/wasm.rs` | WebAssembly object parsing, with `symbolic-debuginfo/src/wasm/parser.rs`. | INV-1, INV-2 |
| `symbolic-debuginfo/src/sourcebundle/` | Source bundle ZIP reader, and the writer covered by an exclusion below. | INV-3, INV-6 |
| `symbolic-debuginfo/src/js.rs` | Extracts source map references and debug IDs from JavaScript. | INV-2 |
| `symbolic-ppdb/src/format/` | Low-level Portable PDB metadata, streams, sequence points, and Source Link. | INV-1, INV-2 |
| `symbolic-il2cpp/src/line_mapping/` | IL2CPP line mapping parsing. The crate is experimental. | INV-2 |
| `symbolic-unreal/src/` | Unreal Engine crash report container, context XML, and logs. Relay parses these from the internet. | INV-2, INV-3 |
| `symbolic-cfi/src/lib.rs` | Converts unwind info from parsed objects into Breakpad CFI records. | INV-2 |

Symbol name and source processing (attacker-controlled strings):

| Path | Purpose | Invariants |
| --- | --- | --- |
| `symbolic-demangle/src/lib.rs` | Demangler front end for C++, MSVC, Rust, Swift, and Objective-C, with recursion and output limits. | INV-1, INV-2 |
| `symbolic-demangle/src/swiftdemangle.cpp` | C shim between Rust and the vendored Swift demangler. | INV-1 |
| `symbolic-demangle/vendor/swift/` | Vendored, locally patched Swift demangler in C++. No Rust memory safety. | INV-1, INV-2 |
| `symbolic-demangle/vendor/msvc-demangler/` | Vendored MSVC demangler in Rust. | INV-2 |
| `symbolic-common/src/sourcelinks.rs` | Matches source paths against Source Link URL patterns. | INV-2 |
| `symbolic-common/src/path.rs` | Path joining and cleaning for paths taken from debug info. | INV-6 |

Cache writers and readers (input semi-trusted, trust Unresolved):

| Path | Purpose | Invariants |
| --- | --- | --- |
| `symbolic-symcache/src/v9/` | SymCache binary format: raw layout, parsing, validation, and lookups. | INV-1, INV-4 |
| `symbolic-symcache/src/writer.rs` | Builds a SymCache from untrusted parsed debug info, with `symbolic-symcache/src/transform/`. | INV-2, INV-3 |
| `symbolic-sourcemapcache/src/` | SourceMapCache writer from untrusted source maps and minified JavaScript, plus the reader. | INV-1, INV-2, INV-4 |
| `symbolic-ppdb/src/cache/` | PortablePdbCache writer and reader. | INV-1, INV-4 |

Shared unsafe code and language bindings:

| Path | Purpose | Invariants |
| --- | --- | --- |
| `symbolic-common/src/cell.rs` | `SelfCell`, which ties a parsed view to the buffer it borrows from. | INV-1 |
| `symbolic-common/src/byteview.rs` | Memory-mapped and owned input buffers. | INV-1 |
| `symbolic-cabi/src/` | C ABI over the Rust crates; panic containment in `symbolic-cabi/src/utils.rs`. | INV-1, INV-5 |
| `py/symbolic/` | Python wrapper over the C ABI, used by Sentry. | INV-5 |
| `symbolic-wasm/src/` | WebAssembly bindings published to npm. | INV-1, INV-3 |

## Security Invariants

- **INV-1** Parsing or reading any input, including a malformed cache file,
  never causes undefined behaviour: no out-of-bounds access, use after free,
  or data race. Unsafe code is confined to `symbolic-cabi`, `Pod` casts in the
  cache format raw modules, `SelfCell` in `symbolic-common/src/cell.rs`, and the
  C++ Swift demangler behind `symbolic_demangle_swift`. Enforced by: Rust
  bounds checks; length-checked `Pod` casts; output buffer size checks in
  `symbolic-demangle/src/swiftdemangle.cpp`.
- **INV-2** Parsing or demangling untrusted input terminates without aborting,
  overflowing the stack, or looping forever. Recursion over input structure is
  either iterative or depth-limited. Enforced by: `MSVC_MAX_RECURSION_DEPTH` and
  the `cpp_demangle` recursion limits in `symbolic-demangle/src/lib.rs`;
  `MaxRecursionDepth` in the vendored Swift demangler, whose asserts fail as errors through `symbolic-demangle/vendor/swift/include/assert.h`;
  `MAX_ABSTRACT_ORIGIN_DEPTH` and `MAX_RESOLVE_FUNCTION_DEPTH` in
  `symbolic-debuginfo/src/dwarf/mod.rs`; `MAX_NESTED_INLINEES` in
  `symbolic-debuginfo/src/function_builder.rs`. Enforced elsewhere by: unknown.
- **INV-3** Memory use is bounded by the input size or by a caller-supplied
  limit. Sizes and counts declared inside the input never drive an unbounded
  allocation or decompression. Enforced by: `max_decompressed_section_size` in
  `symbolic-debuginfo/src/elf.rs`; `max_decompressed_embedded_source_size` in
  `symbolic-debuginfo/src/sourcebundle/mod.rs` and
  `symbolic-debuginfo/src/ppdb.rs`; `Unreal4Crash::parse_with_limit`;
  `MAX_CUSTOM_NODES` and `MAX_DATA_BAG_NODES` in
  `symbolic-unreal/src/context.rs`. Enforced on other decompression paths by:
  unknown.
- **INV-4** Cache readers validate the header, magic, version, and internal
  index consistency before serving lookups, and reject a cache that fails.
  Enforced by: `SymCache::parse` and `symbolic-symcache/src/v9/mod.rs`;
  `SourceMapCache::parse` in `symbolic-sourcemapcache/src/lookup.rs`;
  `PortablePdbCache::parse` in `symbolic-ppdb/src/cache/mod.rs`.
- **INV-5** A Rust panic never unwinds across the C ABI. Every exported
  function converts a panic into an error for the caller. Enforced by:
  `landingpad` and `ffi_fn!` in `symbolic-cabi/src/utils.rs`.
- **INV-6** Parsing reads only the buffers the caller passes in. No reader
  opens files, follows paths, or makes network requests based on input
  content, and no reader extracts archive entries to disk. The source bundle
  writer is the one component that reads files named in debug info. Enforced
  by: unknown (design property).

## Finding Classes

Severity scale follows `warden.toml`: critical, high, medium, low, info.

| Class | Invariant | Reportable | Severity context |
| --- | --- | --- | --- |
| Memory corruption or undefined behaviour from untrusted input, in Rust unsafe code or vendored C++ | INV-1 | Yes | Critical. |
| Memory corruption from a malformed cache file only | INV-1, INV-4 | Yes | Critical by default; high if cache files are confirmed semi-trusted (Unresolved). |
| Abort, stack overflow, or infinite loop on untrusted input | INV-2 | Yes | High. These take down the whole multi-tenant process. |
| Unbounded memory: allocation or decompression sized from an untrusted header, with no limit or with the caller's limit bypassed | INV-3 | Yes | High. |
| Unbounded decompression where a documented limit exists and the caller left it at its unlimited default | INV-3 | Unresolved | Proposed: not reportable in symbolic; the finding belongs to the consumer. Default severity applies until confirmed. |
| Panic on untrusted input that unwinds normally | INV-2 | Yes | Medium (Unresolved; proposed because consumers contain a panic to one task). High if reachable through the C ABI without `landingpad`. |
| Superlinear CPU time on untrusted input that still terminates | INV-2 | Yes | Medium; high if a file within consumer size limits stalls a worker for minutes. |
| Denial of service from a malformed cache file only | INV-2, INV-4 | Unresolved | Proposed: low, since it requires write access to the cache store. Default severity applies until confirmed. |
| Panic unwinding across the C ABI | INV-5 | Yes | High. Undefined behaviour in the host process. |
| Reader opens files, follows paths, or makes requests based on input content | INV-6 | Yes | High. |
| Incorrect but memory-safe symbolication output | — | Unresolved | Proposed: functional bug, not a security finding. Default severity applies until confirmed. |

## Out of Scope, Exclusions, and Accepted Risk

- Code under `examples/`, `symbolic-testutils/`, `symbolic-debuginfo/fuzz/`,
  `symbolic-ppdb/fuzz/`, benches, and test modules — not shipped in any
  published package. Unresolved.
- Files read by `SourceBundleWriter` because the operator's own debug info
  references them — the operator chooses the input and owns the resulting
  bundle. Unresolved. Readers of source bundles remain in scope under INV-6.

## Known Limitations and Compensating Controls

- Resource limits in INV-3 are opt-in and default to unlimited. Consumers set
  them: Symbolicator configures `ParseObjectOptions`, and Relay calls
  `Unreal4Crash::parse_with_limit`. When reviewing a consumer, check that it
  sets them.
- Much of the parsing is done by third-party crates such as goblin, gimli,
  pdb, scroll, cpp_demangle, msvc-demangler, sourcemap, and zip. A bug in one
  of them that is reachable through a symbolic API is a finding here, even if
  the fix lands upstream.
- The vendored Swift demangler is C++ and carries local patches that replace
  upstream asserts and aborts. Changes under `symbolic-demangle/vendor/swift/`
  need the same review as unsafe Rust.
- Fuzz targets exist for object parsing and Portable PDB parsing. They reduce
  but do not rule out INV-1 and INV-2 findings.
