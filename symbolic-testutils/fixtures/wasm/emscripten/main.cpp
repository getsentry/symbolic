#include <emscripten.h>
#include <cstdlib>

// Compile with -g3 so DWARF debug info gets embedded in the .wasm,
// and -Wl,--build-id so the file carries a build_id custom section
// (wasm-split will add one too if it's missing).

extern "C" {

// A normal, non-crashing function — useful to sanity-check the module
// loaded and works before you go testing the crash path.
EMSCRIPTEN_KEEPALIVE
int add(int a, int b) {
    return a + b;
}

// A few stacked frames so the resulting Sentry issue has more than one
// wasm frame to symbolicate — makes it obvious whether the uploaded
// debug file actually worked (you'll see level_three/level_two/level_one
// /crash_me by name instead of "wasm-function[3]" / raw offsets).
static void level_3() {
    abort(); // traps -> Sentry's wasmIntegration captures this cleanly.
             // (Thrown C++ exceptions are NOT reliably captured yet by
             // @sentry/wasm as of this writing, so abort() is the known-good
             // way to produce a test crash.)
}

static void level_2() {
    level_3();
}

static void level_1() {
    level_2();
}

EMSCRIPTEN_KEEPALIVE
int marker() { return 12345; }

EMSCRIPTEN_KEEPALIVE
void crash_me() {
    level_1();
}

} // extern "C"
