include/symbolic.h:
	cbindgen -c cbindgen.toml . -o $@

target/debug/c-tests/demangle: c-tests/demangle.c include/symbolic.h
	mkdir -p target/debug/c-tests
	$(CC) -Iinclude -Ltarget/debug -lsymbolic $< -o $@

ctest: target/debug/c-tests/demangle
	./target/debug/c-tests/demangle

.PHONY: include/symbolic.h ctest
