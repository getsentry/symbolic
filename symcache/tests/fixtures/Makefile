# (ENV) build directory
OUT_DIR ?= build

# path to the docker executable
DOCKER = docker

# build command for cross compiling
BUILD.docker = $(DOCKER) run -it --rm \
	-v $(CURDIR):/workdir \
	-e CROSS_TRIPLE=$@ \
	-e OUT_DIR=$(OUT_DIR) \
	multiarch/crossbuild

# all supported targets
TARGETS = \
	linux \
	osx \
	windows \
	$(NULL)

all: $(TARGETS)

$(TARGETS):
	$(BUILD.docker) make $(OUT_DIR)/$@/main

$(OUT_DIR)/%: main.cpp
	@mkdir -p $(@D)
	$(CXX) -std=c++11 -g -O3 -o $@ main.cpp

.PHONY: all $(TARGETS)
