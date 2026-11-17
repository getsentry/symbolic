// Two functions with identical machine code. The linker folds them into a single
// body, so both symbols point at the same address, while the debug info only
// describes the body that was kept.

__attribute__((noinline)) static void folded_first(void) { __builtin_trap(); }

__attribute__((noinline)) static void folded_second(void) { __builtin_trap(); }

int main(int argc, char **argv) {
    (void)argv;
    if (argc > 1) {
        folded_first();
    }
    folded_second();
    return 0;
}
