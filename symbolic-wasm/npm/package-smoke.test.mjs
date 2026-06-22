// Packaged-artifact smoke test for @sentry/symbolic.
//
// Unlike `wasm-pack test` (which builds its own test glue and never loads what
// we publish), this runs against the *installed* package: it resolves
// `@sentry/symbolic` through the package `exports` map, loads the shipped
// `--target web` glue + wasm via `initSync`, and exercises the public API the
// way a real consumer would. It therefore catches packaging regressions
// (bad `exports`, missing `files[]`, broken `initSync`) that behavior tests miss.
//
// Driven by smoke-test.mjs, which `npm pack`s + installs the tarball into a
// throwaway project, copies this file next to it, and runs `node --test`.
// `SMOKE_FIXTURE` points at a debug-info fixture in the repo.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { test } from "node:test";

const require = createRequire(import.meta.url);

// Bare import -> exercises the `.` export; resolve the wasm -> the
// `./symbolic_bg.wasm` export + `files[]`.
const symbolic = await import("@sentry/symbolic");
const wasmPath = require.resolve("@sentry/symbolic/symbolic_bg.wasm");
symbolic.initSync({ module: readFileSync(wasmPath) });

const data = new Uint8Array(readFileSync(process.env.SMOKE_FIXTURE));

test("the shipped package parses a debug file via the Archive API", () => {
  const archive = new symbolic.Archive(data);
  assert.equal(archive.fileFormat, "elf");
  assert.equal(archive.objectCount, 1);

  const objects = archive.objects();
  assert.equal(objects.length, 1);

  const [object] = objects;
  assert.ok(object.debugId.length >= 32, `unexpected debugId: ${object.debugId}`);
  assert.equal(object.arch, "x86_64");
  assert.equal(object.hasDebugInfo, true);
});

test("Archive.peek detects the format without a full parse", () => {
  assert.equal(symbolic.Archive.peek(data), "elf");
});
