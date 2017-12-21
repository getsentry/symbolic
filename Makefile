all: test

build:
	@cargo build --all

test: cargotest pytest

cargotest:
	@cargo test --all

pytest:
	@pip install pytest > /dev/null
	@pip install -v --editable py && pytest -v py

wheel:
	cd py && python setup.py bdist_wheel

sdist:
	cd py && python setup.py sdist --format=zip

wheel-manylinux:
	docker pull $(IMAGE) > /dev/null
	docker run --rm -it -v $(CURDIR):/work -w /work/py $(IMAGE) sh manylinux.sh

.PHONY: all doc test docker wheel sdist wheel-manylinux
