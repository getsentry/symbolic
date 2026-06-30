#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

emcc main.cpp \
  -O0 \
  -g3 \
  -Wl,--build-id \
  -sEXPORTED_FUNCTIONS=_add,_crash_me \
  -sEXPORTED_RUNTIME_METHODS=ccall,cwrap \
  -fdebug-compilation-dir=/tmp \
  -o index.js

rm index.js
