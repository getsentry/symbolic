DOCKER = docker
BUILD.docker = $(DOCKER) run -it --rm -v $(PWD):/workdir -e CROSS_TRIPLE=$@ multiarch/crossbuild

TARGETS = \
	linux \
	osx \
	windows \
	$(NULL)

BUILDS = $(addprefix build_, $(TARGETS))

all: $(TARGETS)

$(TARGETS):
	$(BUILD.docker) make build_$@

$(BUILDS):
	c++ -std=c++11 -g -O3 -o main main.cpp
