#!/bin/bash
# Build ARM64 ELF fixture for CFI testing
# Requires: aarch64-linux-gnu-gcc

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Compile with debug info and optimization
# -g: Generate debug info (CFI is emitted correctly at any optimization level)
# -O2: Standard optimization, representative of real-world binaries
aarch64-linux-gnu-gcc \
    -g \
    -O2 \
    -o cfi_test \
    cfi_test.c

# Verify the output
file cfi_test
echo "ARM64 fixture generated: cfi_test"
