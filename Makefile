all: test

build:
	cargo build --all

test: styletest cargotest pytest

styletest:
	@rustup component add rustfmt-preview
	cargo fmt -- --write-mode diff

cargotest: build
	cargo test --all

virtualenv:
	@which virtualenv || sudo easy_install virtualenv
	@virtualenv virtualenv

pytest: virtualenv
	@. virtualenv/bin/activate                      ;\
	which pytest || pip install pytest > /dev/null  ;\
	pip install -v --editable py && pytest -v py

wheel: virtualenv
	@. virtualenv/bin/activate                      ;\
	cd py && python setup.py bdist_wheel

sdist: virtualenv
	@. virtualenv/bin/activate                      ;\
	cd py && python setup.py sdist --format=zip

wheel-manylinux:
	docker run --rm -it -v $(CURDIR):/work -w /work/py $(IMAGE) sh manylinux.sh

.PHONY: all build test cargotest pytest wheel sdist wheel-manylinux
