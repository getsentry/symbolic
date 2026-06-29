// Orchestrates the packaged-artifact smoke test (package-smoke.test.mjs).
//
// Run via `npm test` from this directory (and by build-npm.sh / CI). Packs this
// package, installs the tarball into a throwaway project exactly as a consumer
// would, then runs the smoke test from inside it — so the `exports` map,
// `files[]`, and `initSync` path are all exercised against the real published
// artifact (things `wasm-pack test` never loads). Exits non-zero on failure.
//
// This file and package-smoke.test.mjs live in the package dir but are excluded
// from `files[]`, so they are not published.

import { execFileSync } from "node:child_process";
import { cpSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));
const PKG_DIR = here(".");
const FIXTURE = here("../../symbolic-testutils/fixtures/linux/crash.debug");

const run = (cmd, args, opts) =>
  execFileSync(cmd, args, { stdio: "inherit", ...opts });

// 1. Pack this package (leaves the tarball here for the release upload too).
const tarball = execFileSync("npm", ["pack", "--silent"], { cwd: PKG_DIR })
  .toString()
  .trim()
  .split("\n")
  .pop();
console.log(`Packed ${tarball}; smoke-testing the installed package...`);

// 2. Install it into a throwaway consumer project.
const dir = mkdtempSync(join(tmpdir(), "symbolic-smoke-"));
try {
  run("npm", ["init", "-y"], { cwd: dir, stdio: "ignore" });
  run("npm", ["install", "--no-audit", "--no-fund", join(PKG_DIR, tarball)], {
    cwd: dir,
    stdio: "ignore",
  });

  // 3. Run the smoke test from inside the consumer project so `@sentry/symbolic`
  //    resolves to the installed package.
  cpSync(here("./package-smoke.test.mjs"), join(dir, "package-smoke.test.mjs"));
  run("node", ["--test", "package-smoke.test.mjs"], {
    cwd: dir,
    env: { ...process.env, SMOKE_FIXTURE: FIXTURE },
  });
  console.log("packaged-artifact smoke test passed");
} finally {
  rmSync(dir, { recursive: true, force: true });
}
