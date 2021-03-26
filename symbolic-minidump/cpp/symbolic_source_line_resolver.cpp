#include "cpp/symbolic_source_line_resolver.h"

#include <assert.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>

#include <limits>
#include <map>
#include <utility>
#include <vector>

#include "cpp/symbolic_cfi_frame_info.h"
#include "processor/module_factory.h"
#include "processor/tokenize.h"

using std::make_pair;
using std::map;
using std::vector;

#ifdef _WIN32
#ifdef _MSC_VER
#define strtok_r strtok_s
#endif
#define strtoull _strtoui64
#endif

extern "C" {
bool resolver_set_endian(void *resolver, bool is_big_endian);
bool resolver_has_module(void *resolver, const char *name);
void *resolver_find_cfi_frame_info(void *resolver,
                                   const char *module,
                                   uint64_t address);
}

SymbolicSourceLineResolver::SymbolicSourceLineResolver(void *resolver,
                                                       bool is_big_endian) {
    resolver_ = resolver;
    resolver_set_endian(resolver_, is_big_endian);
}

bool SymbolicSourceLineResolver::HasModule(const CodeModule *module) {
    string debug_identifier = module->debug_identifier();
    const char *module_name = debug_identifier.c_str();

    return resolver_has_module(resolver_, module_name);
}

CFIFrameInfo *SymbolicSourceLineResolver::FindCFIFrameInfo(
    const StackFrame *frame) {
    if (frame->module) {
        string debug_identifier = frame->module->debug_identifier();
        const char *module_name = debug_identifier.c_str();
        uint64_t address = frame->instruction - frame->module->base_address();

        void *evaluator =
            resolver_find_cfi_frame_info(resolver_, module_name, address);
        return new SymbolicCFIFrameInfo(evaluator);
    } else {
        return NULL;
    }
}
