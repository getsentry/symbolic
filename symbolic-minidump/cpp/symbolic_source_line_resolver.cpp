#include "cpp/symbolic_source_line_resolver.h"

#include <string.h>

#include "cpp/symbolic_cfi_frame_info.h"
#include "processor/windows_frame_info.h"

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
                                      char **program_string_out,
                                      size_t *programs_string_len_out);
void string_free(char *string);
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

    string_free(function_name);
    string_free(source_file_name);
}

CFIFrameInfo *SymbolicSourceLineResolver::FindCFIFrameInfo(
    const StackFrame *frame) {
    if (frame->module) {
        string debug_identifier = frame->module->debug_identifier();
        const char *module_name = debug_identifier.c_str();
        uint64_t address = frame->instruction - frame->module->base_address();

        void *cfi_frame_info =
            resolver_find_cfi_frame_info(resolver_, module_name, address);
        return new SymbolicCFIFrameInfo(cfi_frame_info);
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
        size_t ps_len;

        if (resolver_find_windows_frame_info(
                resolver_, module_name, address, &type_, &prolog_size,
                &epilog_size, &parameter_size, &saved_register_size,
                &local_size, &max_stack_size, &allocates_base_pointer, &ps,
                &ps_len)) {
            string program_string(ps, ps_len);

            return new WindowsFrameInfo(
                static_cast<google_breakpad::WindowsFrameInfo::StackInfoTypes>(
                    type_),
                prolog_size, epilog_size, parameter_size, saved_register_size,
                local_size, max_stack_size, allocates_base_pointer,
                program_string);
        } else {
            return NULL;
        }
    } else {
        return NULL;
    }
}
