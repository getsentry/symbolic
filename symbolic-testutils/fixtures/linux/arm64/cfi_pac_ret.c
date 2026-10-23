// Functions built with `-mbranch-protection=pac-ret`.
// Non-leaf functions sign the return address in LR (x30) with `paciasp`,
// which the compiler describes with `DW_CFA_AARCH64_negate_ra_state` in CFI.

#include <stdio.h>

__attribute__((noinline)) int callee(int x) {
    printf("callee: %d\n", x);
    return x + 1;
}

int main(void) {
    return callee(1);
}
