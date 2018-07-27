all: test
.PHONY: all

build:
	cargo build --all
.PHONY: build

test: styletest cargotest pytest lint
.PHONY: test

styletest:
	@rustup component add rustfmt-preview
	cargo fmt -- --write-mode diff
.PHONY: styletest

cargotest: build
	cargo test --all
.PHONY: cargotest

virtualenv:
	@which virtualenv || sudo easy_install virtualenv
	@virtualenv virtualenv
.PHONY: virtualenv

pytest: virtualenv
	@. virtualenv/bin/activate                      ;\
	which pytest || pip install pytest > /dev/null  ;\
	pip install -v --editable py && pytest -v py
.PHONY: pytest

wheel: virtualenv
	@. virtualenv/bin/activate                      ;\
	cd py && python setup.py bdist_wheel
.PHONY: wheel

sdist: virtualenv
	@. virtualenv/bin/activate                      ;\
	cd py && python setup.py sdist --format=zip
.PHONY: sdist

lint:
	rustup component add clippy-preview --toolchain nightly
	cargo +nightly clippy --all-features --tests --all -- -D clippy
.PHONY: lint

wheel-manylinux:
	docker run --rm -it -v $(CURDIR):/work -w /work/py $(IMAGE) sh manylinux.sh
.PHONY: wheel-manylinux
