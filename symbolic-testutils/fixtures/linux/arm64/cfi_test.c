// Simple functions that will produce CFI records.
// The compiler typically won't emit explicit RA rules on INIT rows
// since the return address is in LR (x30) by default on ARM64.

void leaf_function(void) {
    // Leaf function - no stack frame needed
    // Should produce CFI INIT without explicit .ra rule
}

int callee(int x) {
    // Non-leaf function that may need to save LR
    return x + 1;
}

int caller(int x) {
    // Function that calls another, needs to save/restore LR
    return callee(x) + callee(x + 1);
}

int recursive(int n) {
    if (n <= 0) return 0;
    return n + recursive(n - 1);
}

int main(void) {
    leaf_function();
    return caller(1) + recursive(5);
}
