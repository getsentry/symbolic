# Sparse dSYM fixture

This is a fixture for a regression test for the issue [#1040](https://github.com/getsentry/symbolic/issues/1040).

Run `./build.sh` on arm64 macOS to rebuild `symbolic-1040.dSYM` from `main.rs`.

The generated dSYM stores `__eh_frame` at its section-header offset while the enclosing `__TEXT` segment has a sparse memory layout. 
