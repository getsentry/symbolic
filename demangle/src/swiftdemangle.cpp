#include "swift/Demangling/Demangle.h"


extern "C" int symbolic_demangle_swift(
    const char *symbol, char *buffer, size_t buffer_length, int simplified)
{
    swift::Demangle::DemangleOptions opts;
    if (simplified) {
        opts = swift::Demangle::DemangleOptions::SimplifiedUIDemangleOptions();
    }

    std::string demangled = swift::Demangle::demangleSymbolAsString(
        llvm::StringRef(symbol), opts);

    if (demangled.size() == 0 || demangled.size() >= buffer_length) {
        return false;
    }

    memcpy(buffer, demangled.c_str(), demangled.size());
    buffer[demangled.size()] = '\0';
    return true;
}

extern "C" int symbolic_demangle_is_swift_symbol(
    const char *symbol)
{
    swift::Demangle::Context DCtx;
    return DCtx.hasSwiftCallingConvention(llvm::StringRef(symbol));
}
