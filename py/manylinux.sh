#!/bin/sh
set -e

# Install dependencies needed by our wheel
yum -y -q -e 0 install gcc libffi-devel

# Upgrade wheel
/opt/python/cp311-cp311/bin/pip install --upgrade wheel
# Milksnake 0.1.6 relies on CFFI.
# Python 3.11 has some known compatibility issues with older versions of CFFI,
# leading to errors during usage of milksnake.
# Upgrade CFFI to latest and fresh version
/opt/python/cp311-cp311/bin/pip install --upgrade cffi

# Install Rust
curl https://sh.rustup.rs -sSf | sh -s -- -y
export PATH=~/.cargo/bin:$PATH

cat >~/.cargo/config <<EOF
[net]
git-fetch-with-cli = true
EOF

/opt/python/cp311-cp311/bin/python setup.py bdist_wheel

# Audit wheels
for wheel in dist/*-linux_*.whl; do
	auditwheel repair "$wheel" -w dist/
	rm "$wheel"
done
