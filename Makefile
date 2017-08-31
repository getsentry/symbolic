all: test doc

doc:
	@cargo doc

test:
	@cd common; cargo test
	@cd demangle; cargo test
	@cd proguard; cargo test
	@cd sourcemap; cargo test
	@cargo test

.PHONY: all doc test
