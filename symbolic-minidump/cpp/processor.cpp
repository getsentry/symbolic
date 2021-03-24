#include "cpp/processor.h"

#include "cpp/data_definitions.h"
#include "cpp/memstream.h"
#include "cpp/mmap_symbol_supplier.h"
#include "cpp/symbolic_source_line_resolver.h"
#include "google_breakpad/processor/minidump.h"
#include "google_breakpad/processor/minidump_processor.h"
#include "google_breakpad/processor/process_state.h"

using google_breakpad::Minidump;
using google_breakpad::MinidumpProcessor;
using google_breakpad::ProcessState;

extern "C" {
void resolver_set_endian(void *resolver, bool is_big_endian);
}

process_state_t *process_minidump(const char *buffer,
                                  size_t buffer_size,
                                  void *resolver_,
                                  int *result_out) {
    if (buffer == nullptr) {
        *result_out = google_breakpad::PROCESS_ERROR_MINIDUMP_NOT_FOUND;
        return nullptr;
    }

    ProcessState *state = new ProcessState();
    if (state == nullptr) {
        *result_out = -1;  // Memory allocation issue
        return nullptr;
    }

    imemstream in(buffer, buffer_size);
    Minidump minidump(in);
    if (!minidump.Read()) {
        *result_out = google_breakpad::PROCESS_ERROR_MINIDUMP_NOT_FOUND;
        delete state;
        return nullptr;
    }

    resolver_set_endian(resolver_, minidump.is_big_endian());
    SymbolicSourceLineResolver *resolver =
        static_cast<SymbolicSourceLineResolver *>(resolver_);

    MinidumpProcessor processor(NULL, &resolver);

    *result_out = processor.Process(&minidump, state);
    return process_state_t::cast(state);
}
