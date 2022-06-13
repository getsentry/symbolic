# Symbolic

[![Build Status](https://github.com/getsentry/symbolic/workflows/CI/badge.svg)](https://github.com/getsentry/symbolic/actions?workflow=CI)
<a href="https://crates.io/crates/symbolic"><img src="https://img.shields.io/crates/v/symbolic.svg" alt=""></a>
<a href="https://pypi.python.org/pypi/Symbolic"><img src="https://img.shields.io/pypi/v/symbolic.svg" alt=""></a>
<a href="https://github.com/getsentry/symbolic/blob/master/LICENSE"><img src="https://img.shields.io/pypi/l/Symbolic.svg" alt=""></a>
[![codecov](https://codecov.io/gh/getsentry/symbolic/branch/master/graph/badge.svg?token=suNHZfbjKW)](https://codecov.io/gh/getsentry/symbolic)

[Symbolic](https://docs.rs/symbolic) is a library written in Rust which is used at
[Sentry](https://sentry.io/) to implement symbolication of native stack traces, sourcemap handling
for minified JavaScript and more. It consists of multiple largely independent crates which are
bundled together into a C and Python library so it can be used independently of Rust.

## What's in the package

Symbolic provides the following functionality:

- Symbolication based on custom cache files (symcache)
- Symbol cache file generators from:
  - Mach, ELF and PE symbol tables
  - Mach and ELF embedded DWARF data
  - PDB CodeView debug information
  - Breakpad symbol files
- Demangling support
  - C++ (GCC, clang and MSVC)
  - Objective C / Objective C++
  - Rust
  - Swift
- JavaScript sourcemap expansion
  - Basic token mapping
  - Heuristics to find original function names based on minified sources
  - Indexed sourcemap to sourcemap merging
- Proguard function mappings
- Generate Breakpad symbol files from Mach, ELF and PDBs
- Convenient C and Python library
- Processing of Unreal Engine 4 native crash reports
  - Extract and process minidumps
  - Expose logs and UE4 context information

## Rust Usage

The Rust crates are published to [Crates.io](https://crates.io/crates/symbolic) and documentation is available on [docs.rs](https://docs.rs/symbolic/latest/symbolic/).

## Python Usage

Symbolic is hosted on [PyPI](https://pypi.python.org/pypi/symbolic). It comes as a library with
prebuilt wheels for linux and macOS. On other operating systems or when using as rust library, you
need to build symbolic manually. It should be compatible with both Python 2 and Python 3.

The python library ships all of the above features in a flat module:

```python
from symbolic import Archive

fat = Archive.open('/path/to/object')
obj = fat.get_object(arch = 'x86_64')
print 'object debug id: {}' % obj.debug_id
```

## C Bindings

Symbolic also offers C bindings, which allow for FFI into arbitrary languages. Have a look at the
the [Symbolic C-ABI readme](symbolic-cabi/README.md) for more information.

## Source Crates

A lot of functionality exposed by this library come from independent Rust crates
for better use:

- [sourcemap](https://github.com/getsentry/rust-sourcemap)
- [proguard](https://github.com/getsentry/rust-proguard)
- [gimli](https://github.com/gimli-rs/gimli)
- [goblin](https://github.com/m4b/goblin)
- [pdb](https://github.com/willglynn/pdb)

## Building and Development

To build the Rust crate, we require the **latest stable Rust**, as well as a C++11 compiler. The
crate is split into a workspace with multiple features, so when running building or running tests
always make sure to pass the `--all` and `--all-features` flags.

```bash
# Check whether the crate compiles
cargo check --all --all-features

# Run Rust tests
cargo test --all --all-features
```

We use `rustfmt` and `clippy` from the latest stable channel for code formatting and linting. To
make sure that these tools are set up correctly and running with the right configuration, use the
following make targets:

```bash
# Format the entire codebase
make format

# Run clippy on the entire codebase
make lint
```

Most likely, new functionality also needs to be added to the Python package. This first requires to
expose new functions in the C ABI. For this, refer to the [Symbolic C-ABI readme](cabi/README.md).

We highly recommend to develop and test the python package in a **virtual environment**. Once the
ABI has been updated and tested, ensure the virtualenv is active and install the package, which
builds the native library. There are two ways to install this:

```bash
# Install the release build, recommended:
pip install --editable ./py

# Install the debug build, faster installation but much slower runtime:
SYMBOLIC_DEBUG=1 pip install --editable ./py
```

For testing, we use ubiquitous `pytest`. Again, ensure that your virtualenv is active and the latest
version of the native library has been installed. Then, run:

```bash
# Run tests manually
pytest ./py/tests

# Creates a new virtualenv, installs the release build and runs tests:
make pytest
```

## Examples

The repository contains a few examples that show how to use `symbolic` to work with debug files and
minidumps. Most of these examples can also be used to extract information from such files or verify
their integrity:

- `dump_cfi`: Writes call frame information from an object file to standard out. The output format
  is Breakpad's `STACK` records.

- `dump_sources`: Creates a source archive from all files referenced by an object file. This is now
  integrated into `sentry-cli difutil bundle-sources`.

- `minidump_stackwalk`: Extracts stack traces from a minidump and symbolicates them. A path to a
  directory containing debug information files can be specified.

- `object_debug`: Prints basic information about the contents of an object file.

- `symcache_debug`: Converts an object file into a symcache and prints its contents. Optionally,
  this can be used to symbolicate a relative address.

- `unreal_engine_crash`: Lists files contained within an Unreal Engine 4 crash archive.

To run these examples, use the `run` script. For example:

```sh
./run minidump_stackwalk mini.dmp /path/to/files
```

## License

Symbolic is licensed under the MIT license. It uses some Apache2 licensed code
from Apple for the Swift demangling.
