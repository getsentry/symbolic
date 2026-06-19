// Smoke test for the built @sentry/symbolic npm package.
//
// Loads the generated wasm bindings (npm/) and exercises every exported
// function against a real debug-info fixture, to catch breakage in the
// JS/wasm boundary that the Rust unit tests cannot reach (serialization,
// the source-bundle writer, Option -> undefined, ...).
//
// Run by build-npm.sh after wasm-opt and in CI; exits non-zero on failure.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));

const wasm = await import(here("./npm/symbolic.js"));
wasm.initSync({ module: readFileSync(here("./npm/symbolic_bg.wasm")) });

const data = new Uint8Array(
  readFileSync(here("../symbolic-testutils/fixtures/linux/crash.debug"))
);

let failures = 0;
const check = (name, cond, detail = "") => {
  if (cond) {
    console.log(`  ok   - ${name}`);
  } else {
    failures += 1;
    console.error(`  FAIL - ${name}${detail ? `: ${detail}` : ""}`);
  }
};

// peek_format
const format = wasm.peek_format(data);
check("peek_format returns a canonical name", format === "elf", `got ${format}`);

// parse_debug_file
const archive = wasm.parse_debug_file(data);
const object = archive.objects?.[0];
check("parse_debug_file returns objects", Array.isArray(archive.objects) && !!object);
check(
  "object exposes a debug_id",
  typeof object?.debug_id === "string" && object.debug_id.length >= 32,
  object?.debug_id
);
check("object reports debug info", object?.has_debug_info === true);
check("object arch/kind are canonical strings", object?.arch === "x86_64" && typeof object?.kind === "string");

// list_source_files
const entries = wasm.list_source_files(data);
const entry = entries?.[0];
check("list_source_files returns one entry per object", Array.isArray(entries) && !!entry);
check("entry lists referenced source paths", Array.isArray(entry?.sources) && entry.sources.length > 0);
check("entry debug_id matches parse_debug_file", entry?.debug_id === object?.debug_id);

// create_source_bundle: empty input -> undefined (nothing bundled)
const empty = wasm.create_source_bundle(data, object.debug_id, "crash.debug", []);
check("create_source_bundle returns undefined when no sources are provided", empty === undefined);

// create_source_bundle: supply content for one referenced path -> a bundle
const pick = entry.sources[0];
const sources = [[pick, new TextEncoder().encode("// smoke-test source\n")]];
const bundle = wasm.create_source_bundle(data, object.debug_id, "crash.debug", sources);
check(
  "create_source_bundle returns bytes when a source is provided",
  bundle instanceof Uint8Array && bundle.length > 0,
  bundle && `len=${bundle.length}`
);

if (failures > 0) {
  console.error(`\nwasm smoke test FAILED (${failures} check(s))`);
  process.exit(1);
}
console.log("\nwasm smoke test passed");
