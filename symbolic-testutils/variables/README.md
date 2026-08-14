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
container working directory). Do not build outside Docker: your local checkout path would be
embedded and the binaries would differ from the committed ones.

Then refresh the snapshots, check the diff is what you expected, and commit the rebuilt binaries
together with the updated snapshots:

```sh
cargo insta test --accept -p symbolic-debuginfo --test test_objects
```

## Adding coverage

Prefer adding a new function over growing an existing one: a new function is a purely additive
snapshot diff, while adding a variable to an existing function rewrites all of its location ranges
(they end at the function's size). Either way, churn stays confined to the touched function.

The snapshots deliberately record what symbolic can *not* do yet, so adding support turns into a
visible snapshot diff instead of a new test someone must remember to write: unresolvable types
show as `Unknown`, and inlinee variables render as nothing at all (`inlining()` forces a
`DW_TAG_inlined_subroutine` even at `-O0`, but symbolic does not yet follow their
`DW_AT_abstract_origin` to the name and type).

Not covered yet, worth adding when the surrounding support lands:

- `PrimitiveTypeEncoding::Address` — no ordinary C type on this target maps to `DW_ATE_address`.
- Variables reduced to a `DW_AT_const_value` instead of a location (`gone` in `optimized_out`) —
  symbolic drops these entirely today: present at `-O0`, absent at `-O2`.
- Non-DWARF formats (PDB, dSYM) once those backends grow variable support.

## The -O2 build

Keeping variables alive under the optimizer needs the scaffolding documented in `variables.c`:
`NOINLINE` on every function, opaque inputs from the `volatile` global, and the `USE(x)` asm
marker (a plain `(void)x` does not survive `-O2`). All of it is harmless at `-O0`. The
type-oriented functions carry no scaffolding, so their blocks render (nearly) empty in the
optimized snapshot — deliberately, as a record of what optimization does to them.

When extending location coverage:

- Multi-range location lists come from a value having to *move* to survive a call: it starts in a
  register the callee is allowed to overwrite, so the compiler relocates it (to a callee-saved
  register or the stack) before the call — one variable, one range per home. But this only happens
  when GCC cannot see the callee's body: for a call to a function in the same file, interprocedural
  register allocation knows which registers the callee *really* touches, and a value can
  legitimately stay put straight through the call — no move, no split. So keeping a variable live
  across a same-file call produces no multi-range coverage; call an external function instead
  (`rand()` in `external_call`), which GCC must assume overwrites everything the ABI allows.
- `-O2` register allocation is stable only under the pinned compiler image, and any edit to a
  function rewrites that function's whole block in the optimized snapshot.
