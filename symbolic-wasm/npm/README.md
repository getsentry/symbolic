# @sentry/symbolic

WebAssembly bindings for [`symbolic`](https://github.com/getsentry/symbolic) —
parse debug information files (Mach-O/dSYM, ELF, PE/PDB, Portable PDB,
WebAssembly, Breakpad, source bundles) and extract their metadata.

Runs anywhere WebAssembly does (Node.js and browsers). The host reads the file
and passes the bytes in — no filesystem access is required inside the module.

**Note**: The NPM package is still experimental and does not yet follow semantic versioning.

## Usage

The package ships `wasm-bindgen` "web"-target glue, so you instantiate the
module explicitly with the bundled `.wasm` bytes:

```js
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { initSync, parse_debug_file, peek_format } from "@sentry/symbolic";

// Resolve and load the bundled wasm, then instantiate (synchronous).
const wasmUrl = new URL(
  "./symbolic_bg.wasm",
  import.meta.resolve("@sentry/symbolic")
);
initSync({ module: await readFile(fileURLToPath(wasmUrl)) });

const bytes = await readFile("./libexample.so");
console.log(peek_format(bytes)); // "elf"
console.log(parse_debug_file(bytes));
// {
//   file_format: "elf",
//   objects: [{ debug_id, code_id, arch, file_format, kind,
//               has_symbols, has_debug_info, has_unwind_info, has_sources }]
// }
```

## API

- `parse_debug_file(data: Uint8Array)` — parse an object file and return its
  archive metadata (one or more objects; a fat Mach-O has one per arch slice).
- `peek_format(data: Uint8Array): string` — detect the format without a full
  parse.

## Notes

- zstd-compressed ELF debug sections are decompressed with the pure-Rust
  [`ruzstd`](https://crates.io/crates/ruzstd) decoder (the `zstd` C library is
  not WASM-compatible). This only matters when enumerating DWARF sources, not
  for metadata extraction.

This package is generated from the `symbolic-wasm` crate. See the
[symbolic repository](https://github.com/getsentry/symbolic) for details.
