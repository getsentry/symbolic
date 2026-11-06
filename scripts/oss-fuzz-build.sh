#!/bin/bash -eu
# Build script for OSS-Fuzz (https://github.com/google/oss-fuzz/tree/master/projects/symbolic),
# run inside the OSS-Fuzz builder image, which provides $CC, $CFLAGS, $OUT, $WORK
# and friends. Kept in this repo so fuzz targets and build changes land together.

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

# Builds every cargo-fuzz crate in the repo (symbolic-*/fuzz). Each fuzz crate
# is its own workspace, so its binaries land in <crate>/fuzz/target.

FIXTURES=$ROOT/symbolic-testutils/fixtures

for fuzz_dir in "$ROOT"/symbolic-*/fuzz; do
  crate_dir=$(dirname "$fuzz_dir")
  pushd "$crate_dir"
  cargo fuzz build -O --debug-assertions
  popd

  out_dir=$fuzz_dir/target/x86_64-unknown-linux-gnu/release
  for target in "$fuzz_dir"/fuzz_targets/*.rs; do
    name=$(basename "${target%.*}")
    cp "$out_dir/$name" "$OUT/"
  done
done

# Seed corpora from the test fixtures. Files are renamed to their hash because
# fixture names repeat across directories. libFuzzer derives max_len from the
# largest seed, so the 1MB cap also bounds input size for the binary formats.
zip_seeds() {
  local name=$1; shift
  local dir=$WORK/seeds/$name
  mkdir -p "$dir"
  find "$@" -type f -size -1024k ! -name '*.md' -print0 | while IFS= read -r -d '' f; do
    cp "$f" "$dir/$(sha1sum "$f" | cut -c1-40)"
  done
  if [ -n "$(ls -A "$dir")" ]; then zip -qj "$OUT/${name}_seed_corpus.zip" "$dir"/*; fi
}

# Sourcemap seeds are "<source>\0<sourcemap>" pairs.
mkdir -p "$WORK/sourcemap_seeds"
find "$FIXTURES/sourcemapcache" -name '*.js.map' -size -1000k | while read -r map; do
  src=${map%.map}
  [ -f "$src" ] || continue
  { cat "$src"; printf '\0'; cat "$map"; } > "$WORK/sourcemap_seeds/$(basename "$src")"
done
zip_seeds fuzz_sourcemapcache "$WORK/sourcemap_seeds"
zip_seeds fuzz_objects "$FIXTURES/linux" "$FIXTURES/macos" "$FIXTURES/windows" "$FIXTURES/wasm" "$FIXTURES/android"
zip_seeds fuzz_ppdb "$FIXTURES/windows" "$FIXTURES/ppdb-sourcelink-sample"
zip_seeds fuzz_symcache_writer "$FIXTURES/linux" "$FIXTURES/macos" "$FIXTURES/windows"
zip_seeds fuzz_unreal "$FIXTURES/unreal"
