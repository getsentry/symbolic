#!/bin/bash
# Replace clang with gcc if needed. Produces overlapping_funcs.dSYM which should be placed in
# symbolic-testutils/fixtures/macos for test_write_functions_overlapping_funcs. Also produces an
# executable which can be ignored. Or run it for a surprise! (It doesn't do anything.)
clang -Weverything main.c -v -g -o overlapping_funcs testachio.c testaroni.c
