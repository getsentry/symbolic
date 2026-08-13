/*
 * Optimized companion to variables.c: built at -O2 to produce the location
 * shapes that never occur at -O0, where every variable is a single
 * whole-function stack slot. Targets: register locations, multi-range
 * location lists, and variables that are optimized out.
 *
 * Keeping a variable alive under -O2 needs scaffolding:
 *   - `NOINLINE` keeps each function a real DWARF entity instead of being
 *     folded into its caller.
 *   - `opaque` is read for input values the optimizer cannot constant-fold,
 *     and written to keep final results observable.
 *   - `USE(x)` requires `x` to exist in a register at that point without
 *     emitting any instructions. A plain `(void)x` would not survive -O2.
 *
 * As in variables.c, prefer adding new functions over growing existing ones,
 * and expect -O2 codegen (register choices, range splits) to be stable only
 * within the pinned compiler image. Rebuild and refresh the snapshot as
 * described in README.md.
 */

#include <stdlib.h>

volatile int opaque;

#define USE(x) __asm__ volatile("" : : "r"(x))
#define NOINLINE __attribute__((noinline))

/*
 * Locals that live purely in registers. -O0 spills every local to the stack,
 * so this is the first coverage of `VariableLocation::Register`.
 */
NOINLINE int registers(int a, int b)
{
    int sum = a + b;
    USE(sum);
    int product = sum * b;
    USE(product);
    return product ^ sum;
}

/*
 * A local that is live across a call in the same translation unit. GCC's
 * interprocedural register allocation knows which registers `registers()`
 * actually clobbers, so values may legitimately stay in call-clobbered
 * registers across the call.
 */
NOINLINE int across_call(int a)
{
    int doubled = a * 2;
    USE(doubled);
    int other = registers(a, doubled);
    return doubled + other;
}

/*
 * Values live across a call to an *external* function, which GCC must assume
 * clobbers all call-clobbered registers: `a` and `kept` start in argument
 * registers and move to callee-saved ones before the call, producing
 * multi-range location lists.
 */
NOINLINE int external_call(int a)
{
    int kept = a + opaque;
    USE(kept);
    int r = rand();
    return kept + r + a;
}

/*
 * `gone` is folded into the return value and never materialized: its DIE
 * carries a constant value instead of a location, and symbolic currently
 * drops it entirely — its absence from the snapshot is the record of that
 * gap.
 */
NOINLINE int optimized_out(int a)
{
    int gone = 42;
    return a + gone;
}

int main(void)
{
    int result = across_call(opaque) + optimized_out(opaque) + external_call(opaque);
    opaque = result;
    return 0;
}
