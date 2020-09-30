#include "swift/Demangling/Demangle.h"

#define SYMBOLIC_SWIFT_FEATURE_RETURN_TYPE 0x1
#define SYMBOLIC_SWIFT_FEATURE_ARGUMENT_TYPES 0x2
#define SYMBOLIC_SWIFT_FEATURE_ARGUMENT_NAMES 0x4
#define SYMBOLIC_SWIFT_FEATURE_ALL 0x7

extern "C" int symbolic_demangle_swift(const char *symbol,
                                       char *buffer,
                                       size_t buffer_length,
                                       int features) {
    swift::Demangle::DemangleOptions opts;

    if (features < SYMBOLIC_SWIFT_FEATURE_ALL) {
        opts = swift::Demangle::DemangleOptions::SimplifiedUIDemangleOptions();
        bool return_type = features & SYMBOLIC_SWIFT_FEATURE_RETURN_TYPE;
        bool argument_types = features & SYMBOLIC_SWIFT_FEATURE_ARGUMENT_TYPES;
        bool argument_names = features & SYMBOLIC_SWIFT_FEATURE_ARGUMENT_NAMES;

        // This option toggles both argument *and* return types (and `throws` declarations).
        opts.ShowFunctionArgumentTypes = return_type || argument_types;
        opts.ShowFunctionArguments = argument_names;
    }

    std::string demangled =
        swift::Demangle::demangleSymbolAsString(llvm::StringRef(symbol), opts);

    if (demangled.size() == 0 || demangled.size() >= buffer_length) {
        return false;
    }

    memcpy(buffer, demangled.c_str(), demangled.size());
    buffer[demangled.size()] = '\0';
    return true;
}

extern "C" int symbolic_demangle_is_swift_symbol(const char *symbol) {
    return swift::Demangle::isSwiftSymbol(symbol);
}
