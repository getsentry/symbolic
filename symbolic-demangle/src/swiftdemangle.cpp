#include "swift/Demangling/Demangle.h"
#include <cstdio>

#define SYMBOLIC_SWIFT_FEATURE_RETURN_TYPE 0x1
#define SYMBOLIC_SWIFT_FEATURE_PARAMETERS 0x2
#define SYMBOLIC_SWIFT_FEATURE_ALL 0x3

extern "C" int symbolic_demangle_swift(const char *symbol,
                                       char *buffer,
                                       size_t buffer_length,
                                       int features) {
    swift::Demangle::DemangleOptions opts;

    if (features < SYMBOLIC_SWIFT_FEATURE_ALL) {
        opts = swift::Demangle::DemangleOptions::SimplifiedUIDemangleOptions();
        bool return_type = features & SYMBOLIC_SWIFT_FEATURE_RETURN_TYPE;
        bool argument_types = features & SYMBOLIC_SWIFT_FEATURE_PARAMETERS;

        opts.ShowFunctionReturnType = return_type;
        opts.ShowFunctionArgumentTypes = argument_types;
    }

    std::string demangled;
    try {
        demangled = swift::Demangle::demangleSymbolAsString(llvm::StringRef(symbol), opts);
    } catch (const std::exception& e) {
        snprintf(buffer, buffer_length, "%s", e.what());
        return 2;
    } catch (...) {
        snprintf(buffer, buffer_length, "%s", "unknown exception");
        return 2;
    }

    if (demangled.size() == 0 || demangled.size() >= buffer_length) {
        return 1;
    }

    memcpy(buffer, demangled.c_str(), demangled.size());
    buffer[demangled.size()] = '\0';
    return 0;
}

extern "C" int symbolic_demangle_is_swift_symbol(const char *symbol) {
    return swift::Demangle::isSwiftSymbol(symbol);
}
