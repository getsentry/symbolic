#!/bin/bash

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
build_dir="$(mktemp -d)"
trap 'rm -rf "$build_dir"' EXIT

cd "$script_dir"
rustc \
    --edition=2024 \
    -C debuginfo=2 \
    -C force-unwind-tables=yes \
    -C opt-level=3 \
    -C panic=abort \
    -C save-temps \
    main.rs \
    -o "$build_dir/symbolic-1040"

rm -rf symbolic-1040.dSYM
dsymutil "$build_dir/symbolic-1040" -o symbolic-1040.dSYM
rm -rf symbolic-1040.dSYM/Contents/Resources/Relocations
