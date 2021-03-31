#include "cpp/symbolic_source_line_resolver.h"

#include <string.h>

#include "cpp/symbolic_cfi_frame_info.h"
#include "processor/windows_frame_info.h"

extern "C" {
bool resolver_set_endian(void *resolver, bool is_big_endian);
bool resolver_has_module(void *resolver, const char *name);
void *resolver_find_cfi_frame_info(void *resolver,
                                   const char *module,
                                   uint64_t address);
bool resolver_find_windows_frame_info(void *resolver,
                                      const char *module,
                                      uint32_t address,
                                      long int *type_out,
                                      uint32_t *prolog_size_out,
                                      uint32_t *epilog_size_out,
                                      uint32_t *parameter_size_out,
                                      uint32_t *saved_register_size_out,
                                      uint32_t *local_size_out,
                                      uint32_t *max_stack_size_out,
                                      bool *allocates_base_pointer_out,
                                      char **program_string_out);

void string_free(char *string);
}

SymbolicSourceLineResolver::SymbolicSourceLineResolver(void *resolver,
                                                       bool is_big_endian)
    : windows_frame_infos_() {
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

WindowsFrameInfo *SymbolicSourceLineResolver::FindWindowsFrameInfo(
    const StackFrame *frame) {
    if (frame->module) {
        string debug_identifier = frame->module->debug_identifier();
        const char *module_name = debug_identifier.c_str();
        uint32_t address = frame->instruction - frame->module->base_address();

        long int type_ = 0;
        uint32_t prolog_size = 0;
        uint32_t epilog_size = 0;
        uint32_t parameter_size = 0;
        uint32_t saved_register_size = 0;
        uint32_t local_size = 0;
        uint32_t max_stack_size = 0;
        bool allocates_base_pointer = false;
        char *ps;

        if (resolver_find_windows_frame_info(
                resolver_, module_name, address, &type_, &prolog_size,
                &epilog_size, &parameter_size, &saved_register_size,
                &local_size, &max_stack_size, &allocates_base_pointer, &ps)) {
            string program_string(ps);
            string_free(ps);

            const WindowsFrameInfo *wfi = new WindowsFrameInfo(
                static_cast<google_breakpad::WindowsFrameInfo::StackInfoTypes>(
                    type_),
                prolog_size, epilog_size, parameter_size, saved_register_size,
                local_size, max_stack_size, allocates_base_pointer,
                program_string);

            windows_frame_infos_.push_back(*wfi);

            return &(windows_frame_infos_.back());
        } else {
            return NULL;
        }
    } else {
        return NULL;
    }
}
