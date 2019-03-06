all: header test release
.PHONY: all

header: include/symbolic.h
.PHONY: header

TESTS = $(patsubst c-tests/%.c, %, $(wildcard c-tests/*.c))

test: build header $(TESTS:%=test-%)
.PHONY: test

test-%: target/debug/c-tests/%
	./$<

build:
	cargo build
.PHONY: build

release:
	cargo build --release
.PHONY: release

clean:
	cargo clean
.PHONY: clean

include/symbolic.h: *.toml src/*.rs
	@touch src/common.rs  # Ensure that rustc outputs something
	RUSTUP_TOOLCHAIN=nightly cbindgen -c cbindgen.toml . -o $@
	@touch include/symbolic.h  # Ensure that we don't build again

target/debug/c-tests/%: c-tests/%.c include/symbolic.h
	@mkdir -p target/debug/c-tests
	$(CC) -Iinclude -Ltarget/debug -lsymbolic $< -o $@
