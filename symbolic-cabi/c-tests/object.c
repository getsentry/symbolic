#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "symbolic.h"

void test_object_open(void) {
    printf("[TEST] open archive from path:\n");

    SymbolicArchive *archive =
        symbolic_archive_open("../symbolic-testutils/fixtures/windows/crash.exe");
    assert(archive != 0);

    SymbolicObject *object = symbolic_archive_get_object(archive, 0);
    assert(object != 0);

    SymbolicStr code_id = symbolic_object_get_code_id(object);
    printf("  code_id:  %.*s\n", (int)code_id.len, code_id.data);

    SymbolicStr debug_id = symbolic_object_get_debug_id(object);
    printf("  debug_id: %.*s\n", (int)debug_id.len, debug_id.data);

    assert(code_id.len > 0);
    assert(strncmp("5ab380779000", code_id.data, code_id.len) == 0);
    assert(debug_id.len > 0);
    assert(strncmp("3249d99d-0c40-4931-8610-f4e4fb0b6936-1", debug_id.data,
                   debug_id.len) == 0);

    symbolic_str_free(&code_id);
    symbolic_str_free(&debug_id);
    symbolic_object_free(object);
    symbolic_archive_free(archive);
    symbolic_err_clear();

    printf("  PASS\n\n");
}

int main() {
    test_object_open();

    return 0;
}
