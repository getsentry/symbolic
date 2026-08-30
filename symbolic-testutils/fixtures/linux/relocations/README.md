`unlinked.o` is an unlinked ELF relocatable object (`ET_REL`) compiled from a source file in a
subdirectory relative to the compilation directory, to reproduce a bug where unapplied ELF
relocations in `.rela.debug_line`/`.rela.debug_info` left every `DW_FORM_line_strp`/`DW_FORM_strp`
offset field reading back as 0 (see the `apply_section_relocations` fix in `elf.rs`).

Generated with:

```sh
mkdir -p subdir
echo 'int example_function(void) { return 0; }' > subdir/example.c
gcc -g -c subdir/example.c -o unlinked.o
```

The exact GCC version doesn't matter -- any compiler that leaves relocatable placeholders for
`DW_FORM_line_strp`/`DW_FORM_strp` fields (confirmed with GCC 13/15 and Clang) reproduces the same
shape. What matters is that the file stays an unlinked `.o` (never run through a linker), since
linking resolves these relocations and would defeat the point of the fixture.
