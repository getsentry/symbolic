#include "google_breakpad/processor/basic_source_line_resolver.h"
#include "google_breakpad/processor/minidump.h"
#include "google_breakpad/processor/minidump_processor.h"
#include "google_breakpad/processor/process_state.h"

#include "cpp/data_definitions.h"
#include "cpp/memstream.h"
#include "cpp/mmap_symbol_supplier.h"
#include "cpp/processor.h"

using google_breakpad::BasicSourceLineResolver;
using google_breakpad::Minidump;
using google_breakpad::MinidumpProcessor;
using google_breakpad::ProcessState;

process_state_t *process_minidump(const char *buffer,
                                  size_t buffer_size,
                                  symbol_entry_t *symbols,
                                  size_t symbol_count,
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

    BasicSourceLineResolver resolver;
    MmapSymbolSupplier supplier(symbol_count, symbols);
    MinidumpProcessor processor(&supplier, &resolver);

    imemstream in(buffer, buffer_size);
    Minidump minidump(in);
    if (!minidump.Read()) {
        *result_out = google_breakpad::PROCESS_ERROR_MINIDUMP_NOT_FOUND;
        return nullptr;
    }

    *result_out = processor.Process(&minidump, state);
    if (*result_out != google_breakpad::PROCESS_OK) {
        delete state;
        return nullptr;
    }

    return process_state_t::cast(state);
}
