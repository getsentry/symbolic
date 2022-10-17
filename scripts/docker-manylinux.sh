#!/usr/bin/env bash
set -e

if [ -z "$TARGET" ]; then
    echo "TARGET is not set"
    exit 1
fi

TARGET_LINKER="CARGO_TARGET_$(echo $TARGET | tr '[:lower:]' '[:upper:]')_UNKNOWN_LINUX_GNU_LINKER"
# Set cargo build arguments
if [[ "x86_64" != "$TARGET" ]]; then
  export CARGO_BUILD_TARGET="${TARGET}-unknown-linux-gnu"
  export "${TARGET_LINKER}"="${TARGET}-linux-gnu-gcc"
fi

# Build docker image with all dependencies for cross compilation
BUILDER_NAME="${BUILDER_NAME:-symbolic-cabi-builder-${TARGET}}"
docker build --build-arg TARGET=${TARGET} -t ${BUILDER_NAME} py/

# run the cross compilation
docker run \
  --rm \
  -w "/work" \
  -v "$(pwd):/work" \
  -e $TARGET_LINKER \
  -e CARGO_BUILD_TARGET \
  ${BUILDER_NAME} \
  bash -c 'cargo build -p symbolic-cabi --release'

# create a wheel for the correct architecture
docker run \
  --rm \
  -w /work/py \
  -v "$(pwd):/work" \
  -e SKIP_SYMBOLIC_LIB_BUILD=1 \
  -e CARGO_BUILD_TARGET \
  quay.io/pypa/manylinux2014_${TARGET} \
  sh manylinux.sh

# Fix permissions for shared directories
USER_ID=$(id -u)
GROUP_ID=$(id -g)
sudo chown -R ${USER_ID}:${GROUP_ID} target/
