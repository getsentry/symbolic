# Variables fixture

Source for the debug info fixtures used by the variable extraction tests, currently
`fixtures/linux/variables` (ELF/DWARF 5, x86-64), asserted by `test_elf_variables` in
`symbolic-debuginfo/tests/test_objects.rs`.

`variables.c` is our fixture which is meant to grow along with symbolic's variable support.

To run the test without rebuilding the snapshot, use:

```sh
cargo test -p symbolic-debuginfo --test test_objects test_elf_variables
```

## Rebuilding

If you make changes to `variables.c`, updating the snapshot takes two steps. First, rebuild the
fixture — run this from *this* directory; it writes `../fixtures/linux/variables`:

```sh
docker run --rm --platform linux/amd64 \
    -v "$PWD/..:/testutils" -w /testutils/variables gcc:14.4.0 ./build.sh
```

Docker pins the compiler, the target architecture and the paths embedded in the debug info
(`DW_AT_comp_dir` comes from the container working directory), so the fixture does not depend on
the machine that built it. Do not run `./build.sh` outside Docker: even with the right GCC
version, your local checkout path would be embedded in the debug info and the binary would differ
from the committed one.

Second, refresh the snapshot and check the diff is what you expected:

```sh
cargo insta test --accept -p symbolic-debuginfo --test test_objects
```

To keep remote up-to-date, commit the rebuilt binary together with the updated snapshot.

## Adding coverage

Prefer adding a new function to `variables.c` over growing an existing one. A new function produces
a purely additive snapshot diff, whereas adding a variable to an existing function rewrites every
variable line in that function: location ranges are printed relative to the function start but end
at its size, so any change to a function's body shifts them all. Reordering declarations is cheap
by comparison — it swaps the affected entries and their stack slots and nothing else.

Either way the churn is confined to the function you touched, so a large diff in one function and
none anywhere else is the expected shape.

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
- `Location::Register` and multi-range location lists — these need an optimized build, since at
  `-O0` GCC spills every local to the stack and every variable gets a single whole-function
  `DW_OP_fbreg` location. Adding an `-O2` variant of the same source is the natural next step, but
  it needs `noinline`/`volatile` scaffolding to stop the optimizer deleting the fixture outright.
- Non-DWARF formats. The same source should build to a PDB and a dSYM once those backends grow
  variable support.
