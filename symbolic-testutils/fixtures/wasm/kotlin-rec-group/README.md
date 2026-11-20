# Kotlin/Wasm multi-type recursion group

`module.wasm` is a 313-byte prefix of a real Kotlin/Wasm module, not a
hand-written binary. Its type section is the first four recursion groups of
`artifacts/demo.wasm` from the Compose Multiplatform repro attached to
[#1090](https://github.com/getsentry/symbolic/issues/1090) (Kotlin 2.4.20).

Those groups hold 17 types. The fourth group is one explicit `rec` of 13 types,
including function types. The module defines one function of type index 4, the
first function type in that group. The body is `unreachable`, so the fixture does
not need the original module's imports. Before the type-index fix, `WasmObject::parse`
rejected this file with `function references unknown type`: the type section has 4
recursion groups and the function names type 4.

The full `demo.wasm` is about 20MB. This prefix is the part that shows the mismatch.
