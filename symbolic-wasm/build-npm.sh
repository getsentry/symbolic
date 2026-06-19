#!/usr/bin/env bash
#
# Build the @sentry/symbolic npm package.
#
# Compiles the symbolic-wasm crate to wasm32-unknown-unknown, generates JS
# bindings with wasm-bindgen (web target), optimizes with wasm-opt, and packs
# the npm tarball into symbolic-wasm/npm/.
#
# Requirements:
#   - Rust toolchain with the wasm32-unknown-unknown target
#   - wasm-bindgen-cli matching the wasm-bindgen crate version
#   - wasm-opt (binaryen) — for size optimization (required)
#   - node + npm (for the smoke test and `npm pack`)
#
# Usage: make npm   (or: bash symbolic-wasm/build-npm.sh)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
NPM_DIR="$SCRIPT_DIR/npm"

echo "Building symbolic-wasm for wasm32-unknown-unknown..."
# Size-optimize the release build via --config overrides (a [profile.release]
# in symbolic-wasm/Cargo.toml would be ignored — it is not the workspace root).
cargo build -p symbolic-wasm --target wasm32-unknown-unknown --release \
  --config 'profile.release.opt-level="z"' \
  --config 'profile.release.debug=false' \
  --config 'profile.release.lto=true' \
  --config 'profile.release.codegen-units=1' \
  --config 'profile.release.panic="abort"'

WASM_IN="$ROOT/target/wasm32-unknown-unknown/release/symbolic_wasm.wasm"

# wasm-bindgen requires the CLI version to exactly match the crate version
# (the bindgen schema is unstable between versions). Derive the locked crate
# version and ensure the matching CLI is installed.
WB_VERSION="$(cargo tree -p symbolic-wasm -i wasm-bindgen --target wasm32-unknown-unknown \
  | grep -oP 'wasm-bindgen v\K[0-9.]+' | head -1)"
echo "wasm-bindgen crate version: ${WB_VERSION}"

if ! command -v wasm-bindgen >/dev/null 2>&1 \
  || [ "$(wasm-bindgen --version | awk '{print $2}')" != "$WB_VERSION" ]; then
  echo "Installing wasm-bindgen-cli ${WB_VERSION}..."
  cargo install -f wasm-bindgen-cli --version "$WB_VERSION"
fi

echo "Generating JS bindings with wasm-bindgen..."
wasm-bindgen \
  --target web \
  --out-name symbolic \
  --out-dir "$NPM_DIR" \
  --omit-default-module-path \
  "$WASM_IN"

BG_WASM="$NPM_DIR/symbolic_bg.wasm"

if ! command -v wasm-opt >/dev/null 2>&1; then
  echo "error: wasm-opt (binaryen) not found — refusing to package an unoptimized wasm" >&2
  exit 1
fi
echo "Optimizing with wasm-opt -Oz..."
wasm-opt -Oz -o "$BG_WASM" "$BG_WASM"

echo "Running wasm smoke test..."
node "$SCRIPT_DIR/smoke-test.mjs"

echo "Packing npm tarball..."
cd "$NPM_DIR"
npm pack

SIZE_KB=$(( $(wc -c < "$BG_WASM") / 1024 ))
echo "Done. symbolic_bg.wasm: ${SIZE_KB} KB; tarball in $NPM_DIR"
