`elf_compressed_gnu` and `elf_compressed_shf` are synthetic ELF files containing compressed sections that claim to have decompressed sizes of 10B. The sections are marked as compressed by
the legacy method of prefixing the name with `z` and by setting the `SHF_COMPRESSED` flag, respectively. Attempting to actually parse these sections will fail because they are malformed.
This can be used to test the `max_decompressed_section_size` parsing option.

These files were created using the `gen_elf.py` script kindly provided by user ornium on HackerOne.
