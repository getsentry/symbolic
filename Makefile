all: test
.PHONY: all

build:
	@cargo build --all --all-features
.PHONY: build

test: styletest cargotest pytest lint
.PHONY: test

styletest:
	@rustup component add rustfmt 2> /dev/null
	@cargo fmt -- --check
.PHONY: styletest

cargotest: build
	cargo test --all --all-features
.PHONY: cargotest

venv: .venv/bin/python
.PHONY: venv

.venv/bin/python: Makefile
	rm -rf .venv
	virtualenv -p python2 .venv

pytest: venv
	@. .venv/bin/activate                           ;\
	which pytest || pip install pytest > /dev/null  ;\
	pip install -v --editable py && pytest -v py
.PHONY: pytest

wheel: venv
	@. .venv/bin/activate                           ;\
	cd py && python setup.py bdist_wheel
.PHONY: wheel

sdist: venv
	@. .venv/bin/activate                           ;\
	cd py && python setup.py sdist --format=zip
.PHONY: sdist

format:
	@rustup component add rustfmt 2> /dev/null
	@cargo fmt
.PHONY: format

lint:
	@rustup component add clippy 2> /dev/null
	@cargo clippy --all-features --tests --all --examples -- -D clippy::all
.PHONY: lint

wheel-manylinux:
	docker run --rm -it -v $(CURDIR):/work -w /work/py $(IMAGE) sh manylinux.sh
.PHONY: wheel-manylinux
