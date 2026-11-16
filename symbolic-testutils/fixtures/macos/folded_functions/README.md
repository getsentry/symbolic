# Folded functions dSYM fixture

Run `./build.sh` on arm64 macOS to rebuild `folded_functions.dSYM` from `main.c`.

`folded_first` and `folded_second` compile to identical machine code, and the linker deduplicates
them into one body. In the resulting dSYM both symbols share one address, but the DWARF only has a
subprogram for the body that was kept. The name of that function must come from the DWARF, so that
it matches the line records of the body.
