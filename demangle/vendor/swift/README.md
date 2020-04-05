# Vendored Swift Demangler

This folder contains a vendored subset of the [Swift Programming Language]. The Swift library is
reduced to the demangler only to reduce the size of this package.

The current version is **Swift 5.0.1**.

## Sentry Modifications

The library has been modified by patches in this order:

1.  `1-arguments.patch`: Adds an option to hide function arguments.
2.  `2-cpp11.patch`: Creates compatibility with C++11 compilers.

## How to Update

1. **Check out the [latest release] of Swift:**
   1. Create a directory that will house swift and its dependencies:
      ```
      $ mkdir swift-source && cd swift-source
      ```
   2. Clone the swift repository into a subdirectory:
      ```
      $ git clone https://github.com/apple/swift.git
      ```
   3. Check out dependencies:
      ```
      $ ./swift/utils/update-checkout --clone-with-ssh
      ```
   4. Check out the release banch of the latest release:
      ```
      $ git checkout swift-5.0.1-RELEASE
      ```
   5. Build the complete swift project (be very patient, this may take long):
      ```
      $ cd swift
      $ ./utils/build-script
      ```
2. **Copy updated sources and headers from the checkout to this library:**
   1. Run the update script in this directory (requires Python 3):
      ```
      $ ./update.py <path-to-swift-workspace>
      ```
   2. Check for modifications.
   3. Commit _"feat(demangle): Import libswift demangle x.x.x"_ before proceeding.
3. **Apply the patches incrementally:**
   1. Apply the [`1-arguments.patch`] and compile with a C++14 compiler, then commit.
   2. Apply the [`2-cpp11.patch`] and fix all merge issues.
   3. Compile with a **C++11** compiler and iterate until it compiles, then commit.
4. **Add tests for new mangling schemes:**
   1. Identify new mangling schemes. Skip if there are no known changes.
   2. Add test cases to [`tests/swift.rs`]
5. **Update Repository metadata**:
   1. Bump the Swift version number in this README.
   2. Check for changes in the license and update the files.
   3. Update the patch files with the commits generated in step 3:
      ```
      $ git show <commit> > 1-arguments.patch
      ```
6. **Create a pull request.**

[swift programming language]: https://github.com/apple/swift
[latest release]: https://github.com/apple/swift/releases/latest/
