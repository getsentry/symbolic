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

test("il2cppLineMapping extracts source_info markers via a provider", () => {
  const archive = new symbolic.Archive(data);
  const [object] = archive.objects();

  // Synthetic Il2cpp C++: a `source_info` marker followed by a code line maps
  // generated C++ line 2 to Game.cs line 42. The provider ignores the path and
  // returns this for every referenced source file.
  const synthetic = new TextEncoder().encode(
    "//<source_info:Game.cs:42>\nint generated = 0;\n"
  );
  let calls = 0;
  const bytes = symbolic.il2cppLineMapping(object, (path) => {
    assert.ok(typeof path === "string" && path.length > 0, "expected a source path");
    calls += 1;
    return synthetic;
  });
  assert.ok(calls > 0, "provider should be called for referenced source files");
  assert.ok(bytes instanceof Uint8Array, "expected JSON mapping bytes");

  const mapping = JSON.parse(new TextDecoder().decode(bytes));
  assert.ok("__debug-id__" in mapping, "expected the __debug-id__ sentinel");
  const [, fileMap] = Object.entries(mapping).find(([k]) => k !== "__debug-id__");
  assert.deepEqual(fileMap, { "Game.cs": { 2: 42 } });

  // Returning a nullish value for every file yields no mapping (undefined).
  assert.equal(symbolic.il2cppLineMapping(object, () => null), undefined);

  // A non-Uint8Array return is rejected rather than silently coerced (e.g. a
  // number would otherwise become a zero-filled buffer).
  assert.throws(() => symbolic.il2cppLineMapping(object, () => 5));
});
