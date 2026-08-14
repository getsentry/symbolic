# Variables fixture

`variables.c` builds two ELF/DWARF 5, x86-64 fixtures in this directory, asserted by
`test_elf_variables` and `test_elf_variables_opt` in `symbolic-debuginfo/tests/test_objects.rs`:

- `variables` (`-O0`): types and variable kinds with whole-function stack locations.
- `variables_opt` (`-O2`): registers, sub-function ranges, and multi-range location lists.

`variables.c` is meant to grow along with symbolic's variable support.

## Rebuilding

If you change `variables.c`, rebuild from *this* directory — it rewrites both binaries next to
the source:

```sh
docker run --rm --platform linux/amd64 \
    -v "$PWD:/fixture" -w /fixture gcc:14.4.0 ./build.sh
```

Docker keeps the binaries reproducible (pinned compiler, architecture, and embedded paths) — don't
build outside it.

The snapshots record absolute addresses and line records, so changes to the source can shift other
functions' addresses. Refresh both from the repository root and review the diff:

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
- Variables optimized down to a `DW_AT_const_value` instead of a location (`gone` in
  `optimized_out`) — symbolic drops these entirely today: present at `-O0`, absent at `-O2`.
- Non-DWARF formats. The same source should build to a PDB and a dSYM once those backends grow
  variable support.
