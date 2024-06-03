The `crash.debug-z{lib,std}` files contain the exact same debug information as the `crash.debug` file they were derived from.

The files were derived using `llvm-objcopy --compress-debug-sections=z{lib,std} crash.debug crash.debug-z{lib,std}` respectively.
