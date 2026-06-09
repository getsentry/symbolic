The `crash.debug-z{lib,std}` files contain the exact same debug information as the `crash.debug` file they were derived from.

The files were derived using `llvm-objcopy --compress-debug-sections=z{lib,std} crash.debug crash.debug-z{lib,std}` respectively.

`dynsyms_only` is an ELF file that only contains a dynamic symbol table obtained using `llvm-objcopy --only-keep-debug --set-section-type .dynstr=3 --set-section-type .dynsym=11 linux-vdso.1.so dynsyms_only`.
