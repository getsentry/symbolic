#include <assert.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#include "symbolic.h"

#define print(x) write(1, x, sizeof(x) - 1)

void test_demangle_success(void) {
    print("[TEST] demangle success path:\n");

    SymbolicStr mangled = symbolic_str_from_cstr(
        "__ZN9backtrace5dylib5Dylib3get28_$u7b$$u7b$closure$u7d$$u7d$"
        "17hc7d4a2b070814ae3E");
    SymbolicStr demangled = symbolic_demangle(&mangled, /* language */ 0);
    printf("  mangled:   %.*s\n", (int)mangled.len, mangled.data);
    printf("  demangled: %.*s\n", (int)demangled.len, demangled.data);

    assert(strncmp("backtrace::dylib::Dylib::get::{{closure}}", demangled.data,
                   demangled.len) == 0);

    symbolic_str_free(&demangled);
    symbolic_err_clear();

    print("  PASS\n\n");
}

void test_demangle_error(void) {
    print("[TEST] demangle error path:\n");

    SymbolicStr invalid_str = symbolic_str_from_cstr("\xff\x23");
    SymbolicStr rv = symbolic_demangle(&invalid_str, /* language */ 0);
    assert(rv.len == 0);

    SymbolicErrorCode code = symbolic_err_get_last_code();
    printf("  error code: %d\n", code);
    SymbolicStr msg = symbolic_err_get_last_message();
    printf("  error message: %.*s\n", (int)msg.len, msg.data);

    assert(code == SYMBOLIC_ERROR_CODE_UNKNOWN);
    assert(strncmp("invalid utf-8 sequence of 1 bytes from index 0", msg.data,
                   msg.len) == 0);

    symbolic_str_free(&msg);
    symbolic_err_clear();
    print("  PASS\n\n");
}

int main() {
    test_demangle_success();
    test_demangle_error();

    return 0;
}
