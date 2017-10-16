#ifndef SENTRY_DATA_DEFINITIONS_H
#define SENTRY_DATA_DEFINITIONS_H

#include "google_breakpad/processor/call_stack.h"
#include "google_breakpad/processor/code_module.h"
#include "google_breakpad/processor/process_state.h"
#include "google_breakpad/processor/stack_frame.h"

#include "cpp/c_mapping.h"

typedef_extern_c(call_stack_t, google_breakpad::CallStack);
typedef_extern_c(code_module_t, google_breakpad::CodeModule);
typedef_extern_c(process_state_t, google_breakpad::ProcessState);
typedef_extern_c(stack_frame_t, google_breakpad::StackFrame);

#endif
