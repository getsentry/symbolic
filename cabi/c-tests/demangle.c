#include <unistd.h>
#include <stdio.h>
#include <assert.h>
#include "symbolic.h"

#define print(x) write(1, x, sizeof(x) - 1)


int main()
{
    print("Success path:\n");
    SymbolicStr mangled = symbolic_str_from_cstr("__ZN9backtrace5dylib5Dylib3get28_$u7b$$u7b$closure$u7d$$u7d$17hc7d4a2b070814ae3E");
    SymbolicStr demangled = symbolic_demangle(&mangled);
    print("  Mangled: ");
    write(1, mangled.data, mangled.len);
    print("\n  Demangled: ");
    write(1, demangled.data, demangled.len);
    symbolic_str_free(&demangled);

    print("\n\nError path:\n");
    symbolic_err_clear();
    SymbolicStr invalid_str = symbolic_str_from_cstr("\xff\x23");
    SymbolicStr rv = symbolic_demangle(&invalid_str);
    assert(rv.len == 0);
    printf("  Error code: %d\n", symbolic_err_get_last_code());
    SymbolicStr msg = symbolic_err_get_last_message();
    print("  Error message: ");
    write(1, msg.data, msg.len);
    print("\n");
    symbolic_str_free(&msg);

    return 0;
}
