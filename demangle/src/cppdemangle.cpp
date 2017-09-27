#include <stdlib.h>
#include <cxxabi.h>


extern "C" int symbolic_demangle_cpp(
    const char *symbol, char **buffer_out)
{
    int status;
    size_t length;
    char *buffer = abi::__cxa_demangle(symbol, 0, &length, &status);
    if (status == 0) {
        *buffer_out = buffer;
        return 1;
    } else {
        free(buffer);
        return 0;
    }
}

extern "C" void symbolic_demangle_cpp_free(char *buf) {
    free(buf);
}
