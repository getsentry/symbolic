target/symbolic.h:
	cbindgen -c cbindgen.toml . -o $@

target/debug/c-tests/demangle: c-tests/demangle.c target/symbolic.h
	mkdir -p target/debug/c-tests
	$(CC) -Itarget -Ltarget/debug -lsymbolic $< -o $@

ctest: target/debug/c-tests/demangle
	./target/debug/c-tests/demangle

.PHONY: header ctest
