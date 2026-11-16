#!/bin/bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
build_dir="$(mktemp -d)"
trap 'rm -rf "$build_dir"' EXIT

cd "$script_dir"
clang \
    -arch arm64 \
    -g \
    -O2 \
    -fdebug-compilation-dir=. \
    -c main.c \
    -o "$build_dir/main.o"
clang \
    -arch arm64 \
    -Wl,-deduplicate \
    "$build_dir/main.o" \
    -o "$build_dir/folded_functions"

rm -rf folded_functions.dSYM
dsymutil "$build_dir/folded_functions" -o folded_functions.dSYM
rm -rf folded_functions.dSYM/Contents/Resources/Relocations
