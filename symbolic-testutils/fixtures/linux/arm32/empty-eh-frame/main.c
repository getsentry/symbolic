__attribute__((noinline)) static int child(int value) {
    return value + 1;
}

void _start(void) {
    volatile int value = child(41);
    (void)value;

    for (;;) {}
}
