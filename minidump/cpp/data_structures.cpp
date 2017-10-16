#include "google_breakpad/processor/stack_frame.h"

#include "cpp/c_string.h"
#include "cpp/data_definitions.h"
#include "cpp/data_structures.h"

using google_breakpad::StackFrame;

void process_state_delete(process_state_t *state) {
  if (state != nullptr) {
    delete process_state_t::cast(state);
  }
}

call_stack_t *const *process_state_threads(process_state_t *state,
                                           size_t *size_out) {
  if (state == nullptr) {
    return nullptr;
  }

  auto *threads = process_state_t::cast(state)->threads();
  if (size_out != nullptr) {
    *size_out = threads->size();
  }

  return reinterpret_cast<call_stack_t *const *>(threads->data());
}

uint32_t call_stack_thread_id(const call_stack_t *stack) {
  return (stack == nullptr) ? 0 : call_stack_t::cast(stack)->tid();
}

stack_frame_t *const *call_stack_frames(const call_stack_t *stack,
                                        size_t *size_out) {
  if (stack == nullptr) {
    return nullptr;
  }

  auto *frames = call_stack_t::cast(stack)->frames();
  if (size_out != nullptr) {
    *size_out = frames->size();
  }

  return reinterpret_cast<stack_frame_t *const *>(frames->data());
}

uint64_t stack_frame_instruction(const stack_frame_t *frame) {
  if (frame == nullptr) {
    return 0;
  }

  return stack_frame_t::cast(frame)->instruction;
}

const code_module_t *stack_frame_module(const stack_frame_t *frame) {
  if (frame == nullptr) {
    return nullptr;
  }

  return code_module_t::cast(stack_frame_t::cast(frame)->module);
}

int stack_frame_trust(const stack_frame_t *frame) {
  if (frame == nullptr) {
    return StackFrame::FRAME_TRUST_NONE;
  }

  return stack_frame_t::cast(frame)->trust;
}

uint64_t code_module_base_address(const code_module_t *module) {
  return code_module_t::cast(module)->base_address();
}

uint64_t code_module_size(const code_module_t *module) {
  return code_module_t::cast(module)->size();
}

char *code_module_code_file(const code_module_t *module) {
  if (module == nullptr) {
    return nullptr;
  }

  return string_from(code_module_t::cast(module)->code_file());
}

char *code_module_code_identifier(const code_module_t *module) {
  if (module == nullptr) {
    return nullptr;
  }

  return string_from(code_module_t::cast(module)->code_identifier());
}

char *code_module_debug_file(const code_module_t *module) {
  if (module == nullptr) {
    return nullptr;
  }

  return string_from(code_module_t::cast(module)->debug_file());
}

char *code_module_debug_identifier(const code_module_t *module) {
  if (module == nullptr) {
    return nullptr;
  }

  return string_from(code_module_t::cast(module)->debug_identifier());
}
