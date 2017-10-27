# Symbolic

<a href="https://pypi.python.org/pypi/Symbolic"><img src="https://img.shields.io/pypi/v/symbolic.svg" alt=""></a>
<a href="https://travis-ci.org/getsentry/symbolic"><img src="https://travis-ci.org/getsentry/symbolic.svg?branch=master" alt=""></a>
<a href="https://github.com/getsentry/symbolic/blob/master/LICENSE"><img src="https://img.shields.io/pypi/l/Symbolic.svg" alt=""></a>

Symbolic is a library written in Rust which is used at [Sentry](https://sentry.io/)
to implement symbolication of native crashes, sourcemap handling of JavaScript
backtraces and more.  It consists of multiple largely independent crates which are
bundled together into a C and Python library so it can be used independently of
Rust.

## What's in the package

Currently it provides the following functionality:

* symbolication based on custom cache files (symcache)
* symcache file generators from:
  * mach and ELF symbol tables
  * mach and ELF embedded DWARF data
* Demangling support
  * Swift
  * C++
  * Rust
* JavaScript sourcemap expansion
  * basic token mapping
  * heuristics to find original function names based on minified sources
  * indexed sourcemap to sourcemap merging
* proguard function mappings
* minidump processing
* convenient C and Python library

## Source crates

A lot of functionality exposed by this library come from independent Rust
crates for better use:

* [rust-sourcemap](https://github.com/getsentry/rust-sourcemap)
* [rust-proguard](https://github.com/getsentry/rust-proguard)
* [gimli](https://github.com/gimli-rs/gimli)
* [goblin](https://github.com/m4b/goblin)

Additionally we use the following C++ libraries to fill in gaps:

* [breakpad](https://chromium.googlesource.com/breakpad/breakpad/)

## License

Symbolic is licensed under the MIT license.  It uses some Apache2 licensed code
from Apple for the Swift demangling.
