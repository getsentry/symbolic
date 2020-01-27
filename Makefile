SYMBOLIC_PYTHON := python3

all: check test
.PHONY: all

check: style lint
.PHONY: check

clean:
	cargo clean
	cargo clean --manifest-path cabi/Cargo.toml
	rm -rf .venv
.PHONY: clean

# Builds

build:
	@cargo +stable build --all --all-features
.PHONY: build

sdist: .venv/bin/python
	cd py && ../.venv/bin/python setup.py sdist --format=zip
.PHONY: sdist

wheel: .venv/bin/python
	cd py && ../.venv/bin/python setup.py bdist_wheel
.PHONY: wheel

wheel-manylinux:
	docker run --rm -it -v $(CURDIR):/work -w /work/py $(IMAGE) sh manylinux.sh
.PHONY: wheel-manylinux

# Tests

test: test-rust test-python
.PHONY: test

test-rust:
	cargo test --all --all-features
.PHONY: test-rust

test-python: .venv/bin/python
	.venv/bin/pip install -U pytest
	.venv/bin/pip install -v --editable py
	.venv/bin/pytest -v py
.PHONY: test-python

# Style checking

style: style-rust style-python
.PHONY: style

style-rust:
	@rustup component add rustfmt --toolchain stable 2> /dev/null
	cargo +stable fmt -- --check
	cd cabi && cargo +stable fmt -- --check
.PHONY: style-rust

style-python: .venv/bin/python
	.venv/bin/pip install -U black
	.venv/bin/black --check py --exclude 'symbolic/_lowlevel*|dist|build|\.eggs'

# Linting

lint: lint-rust lint-python
.PHONY: lint

lint-rust:
	@rustup component add clippy --toolchain stable 2> /dev/null
	cargo +stable clippy --all-features --all --tests --examples -- -D clippy::all
.PHONY: lint-rust

lint-python: .venv/bin/python
	.venv/bin/pip install -U flake8
	.venv/bin/flake8 tests
.PHONY: lint-python

# Formatting

format: format-rust format-python
.PHONY: format

format-rust:
	@rustup component add rustfmt --toolchain stable 2> /dev/null
	cargo +stable fmt
	cd cabi && cargo +stable fmt
.PHONY: format-rust

format-python: .venv/bin/python
	.venv/bin/pip install -U black
	.venv/bin/black py --exclude 'symbolic/_lowlevel*|dist|build|\.eggs'
.PHONY: format-python

# Dependencies

.venv/bin/python: Makefile
	@rm -rf .venv
	@which virtualenv || sudo easy_install virtualenv
	virtualenv -p $(SYMBOLIC_PYTHON) .venv
