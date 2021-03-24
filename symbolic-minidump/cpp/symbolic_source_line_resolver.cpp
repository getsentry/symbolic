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
void resolver_fill_source_line_info(void *resolver,
                                    const char *module,
                                    uint64_t address,
                                    char **function_name_out,
                                    uint64_t *function_base_out,
                                    char **source_file_name_out,
                                    uint64_t *source_line_out);
void *resolver_find_cfi_frame_info(void *resolver,
                                   const char *module,
                                   uint64_t address);
}

bool SymbolicSourceLineResolver::HasModule(const CodeModule *module) {
    string debug_identifier = module->debug_identifier();
    const char *module_name = debug_identifier.c_str();

    return resolver_has_module((void *)this, module_name);
}

void SymbolicSourceLineResolver::FillSourceLineInfo(StackFrame *frame) {
    string debug_identifier = frame->module->debug_identifier();
    const char *module_name = debug_identifier.c_str();
    uint64_t address = frame->instruction - frame->module->base_address();

    char *function_name, *source_file_name;
    uint64_t function_base, source_line;

    resolver_fill_source_line_info((void *)this, module_name, address,
                                   &function_name, &function_base,
                                   &source_file_name, &source_line);

    // frame->function_name = new std::string(function_name);
    // frame->source_file_name = new std::string(source_file_name);
    frame->function_base = function_base;
    frame->source_line = source_line;
}

CFIFrameInfo *SymbolicSourceLineResolver::FindCFIFrameInfo(
    const StackFrame *frame) {
    string debug_identifier = frame->module->debug_identifier();
    const char *module_name = debug_identifier.c_str();
    uint64_t address = frame->instruction - frame->module->base_address();

    void *evaluator =
        resolver_find_cfi_frame_info((void *)this, module_name, address);
    return new SymbolicCFIFrameInfo(evaluator);
}
