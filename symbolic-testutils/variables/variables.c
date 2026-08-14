/*
 * Test fixture for variable extraction from debug info.
 *
 * This single source builds twice (see build.sh): at -O0, where every
 * variable gets one whole-function stack location, and at -O2, where
 * variables live in registers with partial and multi-range location ranges.
 * Each build has its own fixture binary and snapshot.
 *
 * Extending this fixture:
 *   - Prefer adding new functions over growing out existing ones; a new
 *     function is just an addition to the snapshot diff, adding a variable to an
 *     existing function rewrites every variable line in it (because each
 *     location range ends at the function's size).
 *   - Rebuild and refresh the snapshots as described in README.md.
 *
 * Sections marked "not supported yet" render as `Unknown` in the snapshot.
 * That is intentional: the snapshot doubles as a record of what symbolic can
 * and cannot resolve (yet), so adding support shows up as a snapshot diff.
 *
 * The "Optimized locations" functions near the bottom carry liveness
 * scaffolding so -O2 cannot delete them; the type-oriented functions above
 * carry none, so most of their variables are optimized away and their blocks
 * render (nearly) empty in the optimized snapshot. That too is a record.
 */

#include <stdbool.h>
#include <stdlib.h>

/*
 * Liveness scaffolding for the -O2 build; all of it is harmless at -O0.
 */

/* Opaque input/output the optimizer can neither constant-fold nor delete. */
volatile int opaque;

/* Pin `x` to a plain register location at this point, without emitting any
 * code. Data flow alone keeps most values here alive, but leaves GCC free to
 * describe them as computed expressions (`DW_OP_stack_value`), which symbolic
 * drops from the snapshot; the register demand keeps locations simple and
 * visible. A plain `(void)x` would guarantee neither. */
#define USE(x) __asm__ volatile("" : : "r"(x))

/* Same, for SSE registers, where float values live on x86-64; `USE`'s "r"
 * constraint only fits general-purpose registers. */
#define USE_F(x) __asm__ volatile("" : : "x"(x))

/* Keep every function a real DWARF entity instead of being inlined into its
 * caller at -O2. */
#define NOINLINE __attribute__((noinline))

/*
 * Primitive types, as locals. Covers every `PrimitiveTypeEncoding` variant
 * except `Address`, which no ordinary C type maps to (see README.md).
 */
NOINLINE void primitives(void)
{
    signed char sc = -1;
    unsigned char uc = 1;
    short s = -2;
    unsigned short us = 2;
    int i = -3;
    unsigned int u = 3;
    long long ll = -4;
    unsigned long long ull = 4;
    float f = 1.5f;
    double d = 2.5;
    bool b = true;
    float _Complex fc = 1.0f;
}

/*
 * Pointer types, as parameters, so the fixture covers `Kind::Parameter` too.
 *
 * `str`, `any` and `fn` are not resolvable yet: `const char *` points at a
 * `DW_TAG_const_type`, `void *` has no `DW_AT_type` at all, and `int (*)(int)`
 * points at a `DW_TAG_subroutine_type`.
 */
NOINLINE void pointers(int *num, int **num_ptr, const char *str, void *any, int (*fn)(int))
{
}

/*
 * Aggregate types. Not supported yet -- all of these resolve to `Unknown`.
 */
typedef struct {
    int x;
    int y;
} Point;

enum Color {
    RED = 0,
    GREEN = 1,
};

union Value {
    int as_int;
    float as_float;
};

NOINLINE void aggregates(void)
{
    Point point = {1, 2};
    int array[3] = {1, 2, 3};
    enum Color color = GREEN;
    union Value value = {.as_int = 7};
    const int constant = 42;
}

/*
 * Inlined functions. `always_inline` forces inlining even at -O0, which is the
 * only way this fixture can produce a `DW_TAG_inlined_subroutine`.
 *
 * The variables of the inlinee are not supported yet: their concrete DIEs
 * carry only a location and a `DW_AT_abstract_origin` reference, with name and
 * type living on the abstract DIE, which symbolic does not follow for
 * variables. Both `param` and `doubled` have plain frame-base locations at
 * -O0, so `inlined` rendering with no variables at all in the snapshot is
 * purely the missing origin lookup.
 */
static inline __attribute__((always_inline)) int inlined(int param)
{
    int doubled = param * 2;
    return doubled + 1;
}

NOINLINE void inlining(int outer)
{
    int result = inlined(outer);
}

/*
 * Optimized locations. The functions below target the -O2 fixture; at -O0
 * they just add ordinary stack-located variables to the snapshot.
 */

/*
 * Locals that live purely in registers. -O0 spills every local to the stack,
 * so this is the only coverage of `VariableLocation::Register`.
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
 * Float locals live in SSE registers at -O2, which appear as DWARF register
 * numbers 17 and up on x86-64.
 */
NOINLINE float float_registers(float a, float b)
{
    float sum = a + b;
    USE_F(sum);
    float scaled = sum * b;
    USE_F(scaled);
    return scaled - sum;
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
 * multi-range location lists at -O2.
 */
NOINLINE int external_call(int a)
{
    int kept = a + opaque;
    USE(kept);
    int r = rand();
    return kept + r + a;
}

/*
 * At -O2, `gone` is folded into the return value and never materialized: its
 * DIE carries a constant value instead of a location, and symbolic currently
 * drops it entirely -- its absence from the optimized snapshot is the record
 * of that gap.
 */
NOINLINE int optimized_out(int a)
{
    int gone = 42;
    return a + gone;
}

/*
 * A volatile local must keep a stack home even at -O2, giving the optimized
 * snapshot its only frame-base location. Its type renders as `Unknown`: the
 * volatile qualifier wraps it in a `DW_TAG_volatile_type`, which symbolic
 * does not follow yet (like `const` in `pointers()`).
 */
NOINLINE int stack_home(int a)
{
    volatile int slot = a;
    return slot + 1;
}

int main(void)
{
    primitives();
    pointers(0, 0, 0, 0, 0);
    aggregates();
    inlining(5);

    int result = across_call(opaque) + optimized_out(opaque) + external_call(opaque);
    result += stack_home(opaque);
    result += (int)float_registers((float)opaque, (float)opaque);
    opaque = result;
    return 0;
}
