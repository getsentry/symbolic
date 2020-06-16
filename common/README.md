[![Build Status](https://travis-ci.org/getsentry/symbolic.svg?branch=master)](https://travis-ci.org/getsentry/symbolic)

# symbolic-common

Common functionality for `symbolic`.

This crate exposes a set of key types:

 - [`ByteView`]: Gives access to binary data in-memory or on the file system.
 - [`SelfCell`]: Allows to create self-referential types.
 - [`Name`]: A symbol name that can be demangled with the `demangle` feature.
 - [`InstructionInfo`]: A utility type for instruction pointer heuristics.
 - Functions and utilities to deal with paths from different platforms.

## Features

- `serde` (optional): Implements `serde::Deserialize` and `serde::Serialize` for all data types.
  In the `symbolic` crate, this feature is exposed via `common-serde`.

This module is part of the `symbolic` crate.

[`Name`]: https://docs.rs/symbolic/7/symbolic/common/struct.Name.html
[`ByteView`]: https://docs.rs/symbolic/7/symbolic/common/struct.ByteView.html
[`InstructionInfo`]: https://docs.rs/symbolic/7/symbolic/common/struct.InstructionInfo.html
[`SelfCell`]: https://docs.rs/symbolic/7/symbolic/common/struct.SelfCell.html

License: MIT
