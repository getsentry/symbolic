`unlinked.o` is an unlinked ELF relocatable object (`ET_REL`) compiled from a source file in a
subdirectory, to reproduce a bug where unapplied ELF relocations left `DW_FORM_line_strp`/
`DW_FORM_strp` offsets reading back as 0 (see `apply_section_relocations` in `elf.rs`).

The file was obtained using `mkdir -p subdir && echo 'int example_function(void) { return 0; }' > subdir/example.c && gcc -g -c subdir/example.c -o unlinked.o`.
