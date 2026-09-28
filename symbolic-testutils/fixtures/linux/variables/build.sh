#!/bin/sh
# Builds the ELF/DWARF variable fixture from variables.c.
#
# Run this via Docker so the output does not depend on your machine:
#
#   docker run --rm --platform linux/amd64 \
#       -v "$PWD:/fixture" -w /fixture gcc:14.4.0 ./build.sh
#
# See README.md for details.
set -eu

cd "$(dirname "$0")"

file_prefix_map="-ffile-prefix-map=$(pwd -P)=/fixture"
gcc "$file_prefix_map" -g -gdwarf-5 -O0 -o variables variables.c
gcc "$file_prefix_map" -g -gdwarf-5 -O2 -o variables_opt variables.c
