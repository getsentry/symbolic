# Variables fixture

Debug info fixture used by the variable extraction tests: `variables.c` builds to the `variables`
binary in this directory (ELF/DWARF 5, x86-64), asserted by `test_elf_variables` in
`symbolic-debuginfo/tests/test_objects.rs`.

`variables.c` is meant to grow along with symbolic's variable support.

## Rebuilding

If you change `variables.c`, rebuild from *this* directory — it rewrites `variables` next to
the source:

```sh
docker run --rm --platform linux/amd64 \
    -v "$PWD:/fixture" -w /fixture gcc:14.4.0 ./build.sh
```

Docker keeps the binary reproducible (pinned compiler, architecture, and embedded paths) — don't
build outside it.

The snapshot records absolute addresses and line records, so every rebuild changes it. Refresh it
from the repository root and review the diff:

```sh
cargo insta test -p symbolic-debuginfo --test test_objects --accept -- test_elf_variables
```

## Adding coverage

Add whatever exercises the new support to `variables.c`, rebuild, and refresh the snapshot. Since
addresses are absolute, the diff usually also shifts every function placed after the one you
touched; that churn is expected. What to review is that the variables you added show up the way
you expect.

The snapshot deliberately includes types symbolic cannot resolve yet, which show up as `Unknown`.
That is what makes it useful: adding support for a type turns into a visible snapshot diff instead
of requiring someone to remember to write a new test.

The same applies to inlined variables, which currently render as nothing at all rather than as
`Unknown`: `inlining()` forces a `DW_TAG_inlined_subroutine` even at `-O0`, but the variable DIEs
inside it carry only a location plus a `DW_AT_abstract_origin` reference, and symbolic does not yet
follow the origin to the abstract DIE that holds the name and type. The empty `inlined` entry in
the snapshot is the record of that gap; implementing origin-following will make its `param` and
`doubled` appear as a snapshot diff.

Currently not covered, worth adding when the surrounding support lands:

- `PrimitiveTypeEncoding::Address` — no ordinary C type on this target maps to `DW_ATE_address`.
- `VariableLocation::Register` and multi-range location lists — these need an optimized build, since at
  `-O0` GCC spills every local to the stack and every variable gets a single whole-function
  `DW_OP_fbreg` location. Adding an `-O2` variant of the same source is the natural next step, but
  it needs `noinline`/`volatile` scaffolding to stop the optimizer deleting the fixture outright.
- Non-DWARF formats. The same source should build to a PDB and a dSYM once those backends grow
  variable support.
