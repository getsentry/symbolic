# Variables fixture

`variables.c` is the single source for the debug info fixtures (ELF/DWARF 5, x86-64) asserted by
`test_elf_variables` and `test_elf_variables_opt` in `symbolic-debuginfo/tests/test_objects.rs`.
It is compiled twice:

- `fixtures/linux/variables` (`-O0`): types and variable kinds; every variable has a single
  whole-function stack location.
- `fixtures/linux/variables_opt` (`-O2`): locations — registers, sub-function ranges, and
  multi-range location lists.

Run both tests without rebuilding:

```sh
cargo test -p symbolic-debuginfo --test test_objects test_elf_variables
```

## Rebuilding

After changing `variables.c`, rebuild the fixtures — from *this* directory, writing to
`../fixtures/linux/`:

```sh
docker run --rm --platform linux/amd64 \
    -v "$PWD/..:/testutils" -w /testutils/variables gcc:14.4.0 ./build.sh
```

Docker pins the compiler, the target architecture, and the embedded paths (`DW_AT_comp_dir` is the
container working directory).

Then refresh the snapshots, check the diff is what you expected, and commit the rebuilt binaries
together with the updated snapshots:

```sh
cargo insta test --accept -p symbolic-debuginfo --test test_objects
```

## The -O2 build

The `-O2` build relies on the scaffolding documented in `variables.c`: `NOINLINE` on every
function, opaque inputs from the `volatile` global, and the `USE(x)` asm marker. `USE` keeps a
value alive (a plain `(void)x` would not survive `-O2`), but its main job is subtler: it pins the
value to a plain register location the snapshot can assert. Without it, GCC freely describes
values as computed expressions (`DW_OP_stack_value`), which symbolic drops — the ranges silently
vanish from the snapshot. All of the scaffolding is harmless at `-O0`. The type-oriented functions
carry none of it, so their blocks render (nearly) empty in the optimized snapshot — deliberately,
as a record of what optimization does to them.

When extending location coverage, we should be mindful of the following:

- Multi-range location lists come from a value *moving*: to survive a call, it is relocated from
  a register the callee may overwrite to a safe home — one range per home. Same-file calls don't
  force that move (GCC sees which registers the callee really touches and lets values stay put),
  so keep values live across an *external* call instead (`rand()` in `external_call`), which GCC
  must assume overwrites everything.
- Call every function from `main` with arguments derived from `opaque`: constant arguments would
  let GCC specialize the callee despite `NOINLINE`, folding parameters away (sometimes as a
  renamed `.constprop` clone in the snapshot).
- `USE` takes a general-purpose-register value (`"r"`); `USE_F` is the SSE variant (`"x"`) for
  floats. Aggregates would need yet another constraint (e.g. a memory-clobber variant).

## Adding coverage

Prefer adding a new function over growing an existing one: a new function is a purely additive
snapshot diff, while adding a variable to an existing function rewrites all of its location ranges
(they end at the function's size), and for `-O2` all register allocations might shift.

The snapshots deliberately record what symbolic can *not* do yet, so adding support turns into a
visible snapshot diff instead of a new test someone must remember to write: unresolvable types
show as `Unknown`, and inlinee variables render as nothing at all (`inlining()` forces a
`DW_TAG_inlined_subroutine` even at `-O0`, but symbolic currently does not yet follow their
`DW_AT_abstract_origin` to the name and type).

Not covered yet, but worth adding when the surrounding support lands:

- `PrimitiveTypeEncoding::Address` — no ordinary C type on this target maps to `DW_ATE_address`.
- Variables reduced to a `DW_AT_const_value` instead of a location (`gone` in `optimized_out`) —
  symbolic drops these entirely today: present at `-O0`, absent at `-O2`.
- `DW_OP_entry_value` and `DW_OP_stack_value` location entries — already present in the fixture's
  DWARF (e.g. the parameter tails in `float_registers`: "after `0x4`, `a` is the value `xmm0` held
  on entry"), but symbolic drops these entries today, which is why parameters end early in the
  optimized snapshot. Support landing will surface them as a diff without any fixture changes.
- Non-DWARF formats (PDB, dSYM) once those backends grow variable support.
