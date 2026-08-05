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

If you make make changes to `variables.c`, updating the snapshot requires two things to run this
from *this* directory. The following writes `../fixtures/linux/variables`:

```sh
docker run --rm --platform linux/amd64 \
    -v "$PWD/..:/testutils" -w /testutils/variables gcc:14 ./build.sh
```

Docker pins the compiler, the target architecture and the paths embedded in the debug info, so the
fixture does not depend on the machine that built it. On an x86-64 Linux machine with GCC 14 you
can run `./build.sh` directly for the same result.

Next, you refresh the snapshot and check the diff is what you expected:

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

Currently not covered, worth adding when the surrounding support lands:

- `PrimitiveEncoding::Address` — no ordinary C type on this target maps to `DW_ATE_address`.
- `Location::Register` and multi-range location lists — these need an optimized build, since at
  `-O0` GCC spills every local to the stack and every variable gets a single whole-function
  `DW_OP_fbreg` location. Adding an `-O2` variant of the same source is the natural next step, but
  it needs `noinline`/`volatile` scaffolding to stop the optimizer deleting the fixture outright.
- Non-DWARF formats. The same source should build to a PDB and a dSYM once those backends grow
  variable support.
