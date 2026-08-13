#!/bin/sh
# Builds the ELF/DWARF variable fixture from variables.c.
#
# Run this via Docker so the output does not depend on your machine:
#
#   docker run --rm --platform linux/amd64 \
#       -v "$PWD/..:/testutils" -w /testutils/variables gcc:14.4.0 ./build.sh
#
# See README.md for details.
set -eu

cd "$(dirname "$0")"

gcc -g -gdwarf-5 -O0 -o ../fixtures/linux/variables variables.c
gcc -g -gdwarf-5 -O2 -o ../fixtures/linux/variables_opt variables_opt.c
