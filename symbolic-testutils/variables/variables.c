/*
 * Test fixture for variable extraction from debug info.
 *
 * Extending this fixture:
 *   - Prefer adding new functions over growing out existing ones; a new
 *     function is just an addition to the snapshot diff, adding a variable to an
 *     existing function rewrites every variable line in it (because each
 *     location range ends at the function's size).
 *   - Rebuild and refresh the snapshot as described in README.md.
 *
 * Sections marked "not supported yet" render as `Unknown` in the snapshot.
 * That is intentional: the snapshot doubles as a record of what symbolic can
 * and cannot resolve (yet), so adding support shows up as a snapshot diff.
 */

#include <stdbool.h>

/*
 * Primitive types, as locals. Covers every `PrimitiveTypeEncoding` variant
 * except `Address`, which no ordinary C type maps to (see README.md).
 */
void primitives(void)
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
void pointers(int *num, int **num_ptr, const char *str, void *any, int (*fn)(int))
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

void aggregates(void)
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

void inlining(int outer)
{
    int result = inlined(outer);
}

int main(void)
{
    primitives();
    pointers(0, 0, 0, 0, 0);
    aggregates();
    inlining(5);
    return 0;
}
