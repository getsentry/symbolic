#include "google_breakpad/processor/stack_frame.h"
#include "processor/module_factory.h"

#include "cpp/data_definitions.h"
#include "cpp/resolver.h"

using google_breakpad::BasicModuleFactory;
using google_breakpad::StackFrame;

namespace {

// Factory for modules to resolve stack frames.
BasicModuleFactory factory;

// Defines the private nested type BasicSourceLineResolver::Module
using ResolverModule =
    typename std::remove_pointer<decltype(factory.CreateModule(""))>::type;

StackFrame *clone_stack_frame(const StackFrame *frame) {
  if (frame == nullptr) {
    return nullptr;
  }

  auto *clone = new StackFrame();
  if (clone == nullptr) {
    return nullptr;
  }

  // We only need to clone instructions that are not later overwritten by the
  // resolver. Therefore, we assume this is a pristine unresolved frame.
  clone->instruction = frame->instruction;
  clone->module = frame->module;
  clone->trust = frame->trust;

  return clone;
}

}  // namespace

typedef_extern_c(resolver_t, ResolverModule);

void stack_frame_delete(stack_frame_t *frame) {
  if (frame != nullptr) {
    delete stack_frame_t::cast(frame);
  }
}

const char *stack_frame_function_name(const stack_frame_t *frame) {
  if (frame == nullptr) {
    return nullptr;
  }

  return stack_frame_t::cast(frame)->function_name.c_str();
}

const char *stack_frame_source_file_name(const stack_frame_t *frame) {
  if (frame == nullptr) {
    return nullptr;
  }

  return stack_frame_t::cast(frame)->source_file_name.c_str();
}

int stack_frame_source_line(const stack_frame_t *frame) {
  if (frame == nullptr) {
    return 0;
  }

  return stack_frame_t::cast(frame)->source_line;
}

resolver_t *resolver_new(const char *symbol_buffer, size_t buffer_size) {
  if (symbol_buffer == nullptr || buffer_size == 0) {
    return nullptr;
  }

  auto *module = factory.CreateModule("");
  if (module == nullptr) {
    return nullptr;
  }

  module->LoadMapFromMemory(const_cast<char *>(symbol_buffer), buffer_size);
  return resolver_t::cast(module);
}

void resolver_delete(resolver_t *resolver) {
  if (resolver != nullptr) {
    delete resolver_t::cast(resolver);
  }
}

bool resolver_is_corrupt(const resolver_t *resolver) {
  return resolver_t::cast(resolver)->IsCorrupt();
}

stack_frame_t *resolver_resolve_frame(const resolver_t *resolver,
                                      const stack_frame_t *frame) {
  if (resolver == nullptr || frame == nullptr) {
    return nullptr;
  }

  auto *clone = clone_stack_frame(stack_frame_t::cast(frame));
  if (clone == nullptr) {
    return nullptr;
  }

  resolver_t::cast(resolver)->LookupAddress(clone);
  return stack_frame_t::cast(clone);
}
