#!/usr/bin/env bash
set -euo pipefail

root=$(cd -- "$(dirname -- "$0")" && pwd)

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

clang \
    --target=armv7-linux-gnueabihf \
    -g \
    -O1 \
    -funwind-tables \
    -fno-omit-frame-pointer \
    -ffreestanding \
    -fno-stack-protector \
    -c "$root/main.c" \
    -o "$tmp/main.o"

ld.lld \
    -m armelf_linux_eabi \
    -pie \
    -e _start \
    --build-id=sha1 \
    -o "$tmp/combined" \
    "$tmp/main.o"

# Inject a custom empty eh-frame section, since this clang toolchain doesn't produce such an output,
# this is good enough for our tests.
printf '\0\0\0\0' > "$tmp/empty-eh-frame"
llvm-objcopy \
    --add-section .eh_frame="$tmp/empty-eh-frame" \
    --set-section-flags .eh_frame=alloc,load,readonly,data,contents \
    "$tmp/combined" \
    "$tmp/combined-with-eh-frame"

llvm-objcopy --only-keep-debug "$tmp/combined-with-eh-frame" "$root/debuginfo"
llvm-strip -o "$root/executable" "$tmp/combined-with-eh-frame"
