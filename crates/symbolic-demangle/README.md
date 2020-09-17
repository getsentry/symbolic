[![Build Status](https://travis-ci.org/getsentry/symbolic.svg?branch=master)](https://travis-ci.org/getsentry/symbolic)

# symbolic-demangle

Demangling support for various languages and compilers.

Currently supported languages are:

- C++ (GCC-style compilers and MSVC)
- Rust (both `legacy` and `v0`)
- Swift (up to Swift 5.2)
- ObjC (only symbol detection)

As the demangling schemes for the languages are different, the supported demangling features are
inconsistent. For example, argument types were not encoded in legacy Rust mangling and thus not
available in demangled names.

This module is part of the `symbolic` crate and can be enabled via the `demangle` feature.

## Examples

```rust
use symbolic::common::{Language, Name};
use symbolic::demangle::{Demangle, DemangleOptions};

let name = Name::new("__ZN3std2io4Read11read_to_end17hb85a0f6802e14499E");
assert_eq!(name.detect_language(), Language::Rust);
assert_eq!(name.try_demangle(DemangleOptions::default()), "std::io::Read::read_to_end");
```

License: MIT
