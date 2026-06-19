// Smoke test for the built @sentry/symbolic npm package.
//
// Loads the generated wasm bindings (npm/) and exercises the public API
// against a real debug-info fixture, to catch breakage in the JS/wasm
// boundary that the Rust unit tests cannot reach (the Archive/Object classes,
// the source-bundle callback, Option -> undefined, ...).
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

// peek_format (free function)
check("peek_format returns a canonical name", wasm.peek_format(data) === "elf");

// Archive / Object class API
const archive = new wasm.Archive(data);
check("archive exposes fileFormat", archive.fileFormat === "elf");
check("archive exposes objectCount", archive.objectCount >= 1);

const objects = archive.objects();
const object = objects[0];
check("archive.objects() returns Object instances", Array.isArray(objects) && !!object);
check(
  "object exposes a debug_id",
  typeof object.debugId === "string" && object.debugId.length >= 32,
  object.debugId
);
check("object reports debug info", object.hasDebugInfo === true);
check("object arch/kind are canonical strings", object.arch === "x86_64" && typeof object.kind === "string");
check("archive.object(index) matches objects()[index]", archive.object(0).debugId === object.debugId);

// sourceFiles
const sources = object.sourceFiles();
check("sourceFiles returns referenced paths", Array.isArray(sources) && sources.length > 0);

// createSourceBundle via callback: no sources provided -> undefined
const empty = object.createSourceBundle("crash.debug", () => null);
check("createSourceBundle returns undefined when the callback yields nothing", empty === undefined);

// createSourceBundle via callback: provide content for one referenced path
const pick = sources[0];
const content = new TextEncoder().encode("// smoke-test source\n");
const bundle = object.createSourceBundle("crash.debug", (path) => (path === pick ? content : null));
check(
  "createSourceBundle returns bytes when the callback provides a source",
  bundle instanceof Uint8Array && bundle.length > 0,
  bundle && `len=${bundle.length}`
);

// legacy parse_debug_file still works
const info = wasm.parse_debug_file(data);
check("parse_debug_file (legacy) still returns objects", info.objects?.[0]?.debug_id === object.debugId);

if (failures > 0) {
  console.error(`\nwasm smoke test FAILED (${failures} check(s))`);
  process.exit(1);
}
console.log("\nwasm smoke test passed");
