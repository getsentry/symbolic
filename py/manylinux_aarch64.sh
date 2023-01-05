#!/bin/sh
set -xeu

# build a aarch64 libffi
export CC="$TARGET_CC"
export CXX="$TARGET_CXX"
wget https://github.com/libffi/libffi/releases/download/v3.4.4/libffi-3.4.4.tar.gz
tar -xzf libffi-3.4.4.tar.gz
rm libffi-3.4.4.tar.gz
cd libffi-3.4.4
./configure --prefix=/usr/aarch64-unknown-linux-gnu/aarch64-unknown-linux-gnu/sysroot/usr --with-sysroot=/usr/aarch64-unknown-linux-gnu/aarch64-unknown-linux-gnu/sysroot --host=aarch64-unknown-linux-gnu
make
make install
cd ..
rm -rf libffi-3.4.4

# install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
export PATH=~/.cargo/bin:"$PATH"
. "$HOME/.cargo/env"
rustup target add aarch64-unknown-linux-gnu
cat > ~/.cargo/config <<EOF
[net]
git-fetch-with-cli = true
EOF

# setup a python wheel cross-compilation environment
python -m pip install --upgrade pip
python -m pip install crossenv
rm -rf venv
python -m crossenv /opt/python/cp310-cp310/bin/python3 --cc "$TARGET_CC" --cxx "$TARGET_CXX" --sysroot "$TARGET_SYSROOT" --env LIBRARY_PATH= venv
. venv/bin/activate

# now continue wheel building from within the crossenv
# make sure python subprocesses don't find build setuptools
build-pip uninstall -y setuptools

# make sure cffi is part of the build python
build-pip install cffi

# setuptools and wheel must be cross modules so we get aarch64 artifacts
cross-pip install --upgrade setuptools wheel

# finally build wheel
cross-python setup.py bdist_wheel

# audit wheel (use cross-pip so we get an aarch64 auditwheel)
cross-pip install auditwheel
for wheel in dist/*-linux_*.whl; do
  auditwheel repair --plat manylinux_2_28_aarch64 "$wheel" -w dist/
  rm "$wheel"
done
