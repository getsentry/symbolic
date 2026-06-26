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
const ppdbData = new Uint8Array(readFileSync(process.env.SMOKE_FIXTURE_PPDB));

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

test("embeddedPpdb extracts a standalone Portable PDB from a managed PE", () => {
  const [object] = new symbolic.Archive(ppdbData).objects();
  assert.equal(object.fileFormat, "pe");

  const ppdb = object.embeddedPpdb();
  assert.ok(ppdb instanceof Uint8Array, "expected embeddedPpdb() to return bytes");
  assert.equal(ppdb.length, 10540);

  // The extracted bytes are themselves a parseable Portable PDB.
  const ppdbArchive = new symbolic.Archive(ppdb);
  assert.equal(ppdbArchive.fileFormat, "portablepdb");
  assert.equal(ppdbArchive.objectCount, 1);
});

test("embeddedPpdb returns undefined when there is no embedded PPDB", () => {
  // The ELF fixture is not a PE, so it can never carry an embedded PPDB.
  const [object] = new symbolic.Archive(data).objects();
  assert.equal(object.embeddedPpdb(), undefined);
});

test("the debug session enumerates referenced source files", () => {
  const archive = new symbolic.Archive(data);
  const [object] = archive.objects();

  const session = object.debugSession();
  const files = session.files();
  assert.ok(Array.isArray(files), "files() should return an array");
  assert.ok(files.length > 0, "expected at least one referenced source file");
  for (const file of files) {
    assert.ok(file.abs_path_str.length > 0, "expected a non-empty source path");
  }

  // A path the object does not reference resolves to undefined.
  assert.equal(session.sourceByPath("/definitely/not/referenced"), undefined);
});
