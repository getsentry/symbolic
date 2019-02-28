all: header test
.PHONY: all

header: include/symbolic.h
.PHONY: header

test: build header target/debug/c-tests/demangle
	./target/debug/c-tests/demangle
.PHONY: test

build:
	cargo build
.PHONY: build

clean:
	cargo clean
.PHONY: clean

include/symbolic.h: *.toml src/*.rs
	@touch src/common.rs  # Ensure that rustc outputs something
	RUSTUP_TOOLCHAIN=nightly cbindgen -c cbindgen.toml . -o $@
	@touch include/symbolic.h  # Ensure that we don't build again

target/debug/c-tests/demangle: c-tests/demangle.c include/symbolic.h
	mkdir -p target/debug/c-tests
	$(CC) -Iinclude -Ltarget/debug -lsymbolic $< -o $@
