# Symbolic

<a href="https://travis-ci.org/getsentry/symbolic"><img src="https://travis-ci.org/getsentry/symbolic.svg?branch=master" alt=""></a>
<a href="https://crates.io/crates/symbolic"><img src="https://img.shields.io/crates/v/symbolic.svg" alt=""></a>
<a href="https://pypi.python.org/pypi/Symbolic"><img src="https://img.shields.io/pypi/v/symbolic.svg" alt=""></a>
<a href="https://github.com/getsentry/symbolic/blob/master/LICENSE"><img src="https://img.shields.io/pypi/l/Symbolic.svg" alt=""></a>

Symbolic is a library written in Rust which is used at
[Sentry](https://sentry.io/) to implement symbolication of native stack traces,
sourcemap handling for minified JavaScript and more. It consists of multiple
largely independent crates which are bundled together into a C and Python
library so it can be used independently of Rust.

## What's in the package

Currently it provides the following functionality:

* Symbolication based on custom cache files (symcache)
* Symbol cache file generators from:
  * Mach and ELF symbol tables
  * Mach and ELF embedded DWARF data
  * Breakpad Symbol files
* Demangling support
  * Swift
  * C++
  * Rust
* JavaScript sourcemap expansion
  * Basic token mapping
  * Heuristics to find original function names based on minified sources
  * Indexed sourcemap to sourcemap merging
* Proguard function mappings
* Minidump / Breakpad processing
  * Generate Breakpad symbol files from Mach and ELF
  * Process Minidumps to resolve process state
* Convenient C and Python library

## Source crates

A lot of functionality exposed by this library come from independent Rust crates
for better use:

* [rust-sourcemap](https://github.com/getsentry/rust-sourcemap)
* [rust-proguard](https://github.com/getsentry/rust-proguard)
* [gimli](https://github.com/gimli-rs/gimli)
* [goblin](https://github.com/m4b/goblin)

Additionally we use the following C++ libraries to fill in gaps:

* [breakpad](https://chromium.googlesource.com/breakpad/breakpad/)

## License

Symbolic is licensed under the MIT license. It uses some Apache2 licensed code
from Apple for the Swift demangling.
