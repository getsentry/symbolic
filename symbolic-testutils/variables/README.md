# Variables fixture

Sources for the debug info fixtures (ELF/DWARF 5, x86-64) used by the variable extraction tests in
`symbolic-debuginfo/tests/test_objects.rs`:

- `variables.c` → `fixtures/linux/variables` (`-O0`), asserted by `test_elf_variables`. Covers
  types and variable kinds; at `-O0` every variable has a single whole-function stack location.
- `variables_opt.c` → `fixtures/linux/variables_opt` (`-O2`), asserted by `test_elf_variables_opt`.
  Covers locations: registers, sub-function ranges, and multi-range location lists.

Both fixtures are meant to grow along with symbolic's variable support.

To run the test without rebuilding the snapshot, use:

```sh
cargo test -p symbolic-debuginfo --test test_objects test_elf_variables
```

## Rebuilding

If you make changes to the C sources, updating the snapshots takes two steps. First, rebuild the
fixtures — run this from *this* directory; it writes them under `../fixtures/linux/`:

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
- Variables optimized down to a `DW_AT_const_value` instead of a location (`gone` in
  `variables_opt.c`) — symbolic currently drops these entirely, so they do not even appear as an
  empty entry. When support lands, `gone` will show up as a snapshot diff.
- Non-DWARF formats. The same source should build to a PDB and a dSYM once those backends grow
  variable support.

## The optimized variant

`variables_opt.c` is built at `-O2` to produce the location shapes that never occur at `-O0`:
register locations, ranges shorter than the function, and location lists with multiple entries.
Keeping a variable alive under the optimizer needs scaffolding, documented in the file itself; the
short version is `NOINLINE` on every function, opaque inputs read from a `volatile` global, and a
`USE(x)` asm marker — a plain `(void)x` cast does not survive `-O2`.

Two things to know when extending it:

- A call to a function in the same file does *not* force values out of call-clobbered registers:
  GCC's interprocedural register allocation sees the callee's real clobbers. Multi-range lists
  come from calls to external functions (`rand()` in `external_call`), which GCC must assume
  clobber everything.
- `-O2` register allocation is only stable under the pinned compiler image. Expect any edit to an
  existing function to rewrite that function's whole snapshot block — the per-function churn
  containment still holds, but within a function, ranges and register numbers move freely.
