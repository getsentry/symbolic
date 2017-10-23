#ifndef SENTRY_RESOLVER_H
#define SENTRY_RESOLVER_H

#include <cstddef>
#include "cpp/data_structures.h"

#ifdef __cplusplus
extern "C" {
#endif

/// Source Line Resolver based on Breakpad's BasicSourceLineResolver. This class
/// handles Breakpad symbol files and resolves source code locations for stack
/// frames.
///
/// To interact with the resolver, use the resolver_* family of functions.
struct resolver_t;

/// Releases memory of a stack frame. Assumes ownership of the pointer.
void stack_frame_delete(stack_frame_t *frame);

/// Returns a weak pointer to the function name of the instruction. Can be empty
/// before running the resolver or if debug symbols are missing.
const char *stack_frame_function_name(const stack_frame_t *frame);

/// Returns a weak pointer to the source code file name in which the
/// instruction was declared. Can be empty before running the resolver or if
/// debug symbols are missing.
const char *stack_frame_source_file_name(const stack_frame_t *frame);

/// Returns the source code line at which the instruction was declared. Can
/// be empty before running the resolver or if debug symbols are missing.
int stack_frame_source_line(const stack_frame_t *frame);

/// Creates a new source line resolver instance and returns an owning pointer
/// to it. Symbols are loaded from a buffer containing symbols in ASCII format.
///
/// Release memory of this resolver with the resolver_delete function.
resolver_t *resolver_new(const char *symbol_buffer, size_t buffer_size);

/// Releases memory of a resolver object. Assumes ownership of the pointer.
void resolver_delete(resolver_t *resolver);

/// Returns whether the loaded symbol file was corrupt or can be used for
/// symbol resolution.
bool resolver_is_corrupt(const resolver_t *resolver);

/// Tries to locate the frame's instruction in the loaded code modules. Returns
/// an owning pointer to a new resolved stack frame instance. If no  symbols can
/// be found for the frame, a clone of the input is returned.
///
/// This method expects a weak pointer to a frame. Release memory of this frame
/// with the stack_frame_delete function.
stack_frame_t *resolver_resolve_frame(const resolver_t *resolver,
                                      const stack_frame_t *frame);

#ifdef __cplusplus
}
#endif

#endif
