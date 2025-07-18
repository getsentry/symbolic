#!/bin/sh
set -e

# Install dependencies needed by our wheel
yum -y -q -e 0 install gcc libffi-devel

# upgrade wheel
/opt/python/cp311-cp311/bin/pip install --upgrade wheel

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
