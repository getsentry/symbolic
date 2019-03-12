#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "symbolic.h"

void test_arch_is_known(void) {
    printf("[TEST] arch_is_known:\n");

    SymbolicStr arch1 = symbolic_str_from_cstr("x86");
    bool is_known1 = symbolic_arch_is_known(&arch1);
    assert(symbolic_err_get_last_code() == SYMBOLIC_ERROR_CODE_NO_ERROR);
    printf("  'x86' is known: %s\n", is_known1 ? "true" : "false");

    SymbolicStr arch2 = symbolic_str_from_cstr("amd64");
    bool is_known2 = symbolic_arch_is_known(&arch2);
    assert(symbolic_err_get_last_code() == SYMBOLIC_ERROR_CODE_NO_ERROR);
    printf("  'amd64' is known: %s\n", is_known2 ? "true" : "false");

    SymbolicStr arch3 = symbolic_str_from_cstr("foo");
    bool is_known3 = symbolic_arch_is_known(&arch3);
    assert(symbolic_err_get_last_code() == SYMBOLIC_ERROR_CODE_NO_ERROR);
    printf("  'foo' is known: %s\n", is_known3 ? "true" : "false");

    assert(is_known1);
    assert(is_known2);
    assert(!is_known3);

    symbolic_err_clear();
    printf("  PASS\n\n");
}

void test_normalize_arch(void) {
    printf("[TEST] normalize arch success case:\n");
    SymbolicStr arch = symbolic_str_from_cstr("amd64");
    SymbolicStr normalized = symbolic_normalize_arch(&arch);
    assert(symbolic_err_get_last_code() == SYMBOLIC_ERROR_CODE_NO_ERROR);

    printf("  arch:       %.*s\n", (int)arch.len, arch.data);
    printf("  normalized: %.*s\n", (int)normalized.len, normalized.data);

    assert(strncmp("x86_64", normalized.data, normalized.len) == 0);

    symbolic_err_clear();
    printf("  PASS\n\n");
}

int main() {
    test_arch_is_known();
    test_normalize_arch();

    return 0;
}
