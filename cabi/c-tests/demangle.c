#include <unistd.h>
#include <stdio.h>
#include <assert.h>
#include "symbolic.h"


int main()
{
    const char *mangled = "__ZN9backtrace5dylib5Dylib3get28_$u7b$$u7b$closure$u7d$$u7d$17hc7d4a2b070814ae3E";
    char *demangled = symbolic_demangle(mangled);
    printf("Mangled: %s\n", mangled);
    printf("Demangled: %s\n", demangled);
    symbolic_cstr_free(demangled);

    symbolic_err_clear();
    char *rv = symbolic_demangle("\xff\x23");
    assert(rv == 0);
    printf("Error code: %d\n", symbolic_err_get_last_code());
    SymbolicStr msg = symbolic_err_get_last_message();
    printf("Error message: ");
    fflush(stdout);
    write(1, msg.data, msg.len);
    printf("\n");
    symbolic_str_free(&msg);

    return 0;
}
