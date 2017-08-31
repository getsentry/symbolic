all: test doc

doc:
	@cargo doc

test:
	@cargo test --all

.PHONY: all doc test
