all: build test

build:
	@cargo build --all

test: build
	@cargo test --all

wheel:
	cd py && python setup.py bdist_wheel

wheel-manylinux:
	docker run --rm -it -v $(CURDIR):/work -w /work/py $(IMAGE) sh manylinux.sh

.PHONY: all doc test docker wheel wheel-manylinux
