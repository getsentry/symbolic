# Vendored msvc-demangler

This folder contains a vendored version of [msvc-demangler](https://github.com/mstange/msvc-demangler-rust) which contains changes that have not been upstreamed yet. The goal is for the changes to either be upstreamed or for the upstream to fix the underling issue eventually so we can stop vendoring.

## Sentry Modifications

The library has been modified by adding a max recursion limit.
