#ifndef SENTRY_PROCESSOR_H
#define SENTRY_PROCESSOR_H

#include <cstddef>
#include "cpp/data_structures.h"

#ifdef __cplusplus
extern "C" {
#endif

/// Data transfer object for symbols in memory
struct symbol_entry_t {
    /// The debug identifier of the code module these symbols are for
    const char *debug_identifier;

    /// Size of the buffer inside symbol_data
    const size_t symbol_size;

    /// Raw data of the symbol file passed to the symbolizer
    const char *symbol_data;
};

/// Reads a minidump from a memory buffer and processes it. Returns an owning
/// pointer to a process_state_t struct that contains loaded code modules and
/// call stacks of all threads of the process during the crash.
///
/// Processing the minidump can fail if the buffer is corrupted or does not
/// exit. The function will return NULL and an error code in result_out.
///
/// Release memory of the process state with process_state_delete.
process_state_t *process_minidump(const char *buffer,
                                  size_t buffer_size,
                                  symbol_entry_t *symbols,
                                  size_t symbol_count,
                                  int *result_out);

#ifdef __cplusplus
}
#endif

#endif
