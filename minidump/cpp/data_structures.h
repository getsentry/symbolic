#ifndef SENTRY_DATA_STRUCTURES_H
#define SENTRY_DATA_STRUCTURES_H

#include <cstdbool>
#include <cstddef>
#include <cstdint>

#ifdef __cplusplus
extern "C" {
#endif

/// Structure holding all stack frames in a certain thread. Use the call_stack_*
/// family of functions to interact with a call stack.
struct call_stack_t;

/// Carries information about the code module loaded into the process. The field
/// debug_identifier contains the UUID of this module. Use the code_module_*
/// family of functions to interact with a code module.
struct code_module_t;

/// Snapshot of the state of a process during its crash. This object is obtained
/// by processing Minidumps using the process_* family of functions. To interact
/// with ProcessStates use the process_state_* family of functions.
struct process_state_t;

/// Contains information from the stackdump, especially the frame's instruction
/// pointer. After being processed by a resolver, this struct also contains
/// source code locations and code offsets.
struct stack_frame_t;

/// Information about the CPU and OS on which a minidump was generated.
struct system_info_t;

/// Structure holding the name and value of a CPU register.
struct regval_t {
    /// The register name as specified by the CPU architecture.
    const char *name;
    /// The register value (lowest bits if smaller than 8 bytes).
    uint64_t value;
    /// Size of the register value in bytes.
    uint8_t size;
};

/// Releases memory of a process state struct. Assumes ownership of the pointer.
void process_state_delete(process_state_t *state);

/// Returns a weak pointer to the list of threads in the minidump. Each thread
/// is represented by the call stack structure. The number of threads is
/// returned in the size_out parameter.
call_stack_t *const *process_state_threads(process_state_t *state,
                                           size_t *size_out);

/// The index of the thread that requested a dump be written in the
/// threads vector.  If a dump was produced as a result of a crash, this
/// will point to the thread that crashed.  If the dump was produced as
/// by user code without crashing, and the dump contains extended Breakpad
/// information, this will point to the thread that requested the dump.
/// If the dump was not produced as a result of an exception and no
/// extended Breakpad information is present, this field will be set to -1,
/// indicating that the dump thread is not available.
int32_t process_state_requesting_thread(const process_state_t *state);

/// The time-date stamp of the minidump (time_t format)
uint64_t process_state_timestamp(const process_state_t *state);

/// True if the process crashed, false if the dump was produced outside
/// of an exception handler.
bool process_state_crashed(const process_state_t *state);

/// If the process crashed, and if crash_reason implicates memory,
/// the memory address that caused the crash.  For data access errors,
/// this will be the data address that caused the fault.  For code errors,
/// this will be the address of the instruction that caused the fault.
uint64_t process_state_crash_address(const process_state_t *state);

/// If the process crashed, the type of crash.  OS- and possibly CPU-
/// specific.  For example, "EXCEPTION_ACCESS_VIOLATION" (Windows),
/// "EXC_BAD_ACCESS / KERN_INVALID_ADDRESS" (Mac OS X), "SIGSEGV"
/// (other Unix).
///
/// The return value is an owning pointer. Release memory with string_delete.
char *process_state_crash_reason(const process_state_t *state);

/// If there was an assertion that was hit, a textual representation
/// of that assertion, possibly including the file and line at which
/// it occurred.
///
/// The return value is an owning pointer. Release memory with string_delete.
char *process_state_assertion(const process_state_t *state);

/// Returns a weak pointer to OS and CPU information.
const system_info_t *process_state_system_info(const process_state_t *state);

/// A string identifying the operating system, such as "Windows NT",
/// "Mac OS X", or "Linux".  If the information is present in the dump but
/// its value is unknown, this field will contain a numeric value.  If
/// the information is not present in the dump, this field will be empty.
///
/// The return value is an owning pointer. Release memory with string_delete.
char *system_info_os_name(const system_info_t *info);

/// A string identifying the version of the operating system, such as
/// "5.1.2600 Service Pack 2" or "10.4.8 8L2127".  If the dump does not
/// contain this information, this field will be empty.
///
/// The return value is an owning pointer. Release memory with string_delete.
char *system_info_os_version(const system_info_t *info);

/// A string identifying the basic CPU family, such as "x86" or "ppc".
/// If this information is present in the dump but its value is unknown,
/// this field will contain a numeric value.  If the information is not
/// present in the dump, this field will be empty.  The values stored in
/// this field should match those used by MinidumpSystemInfo::GetCPU.
///
/// The return value is an owning pointer. Release memory with string_delete.
char *system_info_cpu_family(const system_info_t *info);

/// A string further identifying the specific CPU, such as
/// "GenuineIntel level 6 model 13 stepping 8".  If the information is not
/// present in the dump, or additional identifying information is not
/// defined for the CPU family, this field will be empty.
///
/// The return value is an owning pointer. Release memory with string_delete.
char *system_info_cpu_info(const system_info_t *info);

/// The number of processors in the system.  Will be greater than one for
/// multi-core systems.
uint32_t system_info_cpu_count(const system_info_t *info);

/// Returns the thread identifier of this callstack.
uint32_t call_stack_thread_id(const call_stack_t *stack);

/// Returns a weak pointer to the list of frames in a call stack. Each frame is
/// represented by the stack frame structure. The number of frames is returned
/// in the size_out parameter.
stack_frame_t *const *call_stack_frames(const call_stack_t *stack,
                                        size_t *size_out);

// Return the actual return address, as saved on the stack or in a
// register. See the comments for 'stack_frameinstruction', below,
// for details.
uint64_t stack_frame_return_address(const stack_frame_t *frame);

/// Returns the program counter location as an absolute virtual address.
///
/// - For the innermost called frame in a stack, this will be an exact
///   program counter or instruction pointer value.
///
/// - For all other frames, this address is within the instruction that
///   caused execution to branch to this frame's callee (although it may
///   not point to the exact beginning of that instruction). This ensures
///   that, when we look up the source code location for this frame, we
///   get the source location of the call, not of the point at which
///   control will resume when the call returns, which may be on the next
///   line. (If the compiler knows the callee never returns, it may even
///   place the call instruction at the very end of the caller's machine
///   code, such that the "return address" (which will never be used)
///   immediately after the call instruction is in an entirely different
///   function, perhaps even from a different source file.)
///
/// On some architectures, the return address as saved on the stack or in
/// a register is fine for looking up the point of the call. On others, it
/// requires adjustment. ReturnAddress returns the address as saved by the
/// machine.
///
/// Use stack_frame_trust to obtain how trustworthy this instruction is.
uint64_t stack_frame_instruction(const stack_frame_t *frame);

/// Returns a weak pointer to the code module that hosts the instruction of the
/// stack framme. This function can return null for some frames.
const code_module_t *stack_frame_module(const stack_frame_t *frame);

/// Returns how well the instruction pointer derived during
/// stack walking is trusted. Since the stack walker can resort to
/// stack scanning, it can wind up with dubious frames.
/// In rough order of "trust metric".
int stack_frame_trust(const stack_frame_t *frame);

/// Returns an owned pointer to a list of register values of this frame. The
/// number of values is returned in the size_out parameter.
regval_t *stack_frame_registers(const stack_frame_t *frame,
                                uint32_t family,
                                size_t *size_out);

/// Releases memory of a regval struct. Assumes ownership of the pointer.
void regval_delete(regval_t *regval);

/// Returns the base address of this code module as it was loaded by the
/// process. (uint64_t)-1 on error.
uint64_t code_module_base_address(const code_module_t *module);

/// The size of the code module. 0 on error.
uint64_t code_module_size(const code_module_t *module);

/// Returns the path or file name that the code module was loaded from.
///
/// The return value is an owning pointer. Release memory with string_delete.
char *code_module_code_file(const code_module_t *module);

/// An identifying string used to discriminate between multiple versions and
/// builds of the same code module.  This may contain a uuid, timestamp,
/// version number, or any combination of this or other information, in an
/// implementation-defined format.
///
/// The return value is an owning pointer. Release memory with string_delete.
char *code_module_code_identifier(const code_module_t *module);

/// Returns the filename containing debugging information of this code
/// module.  If debugging information is stored in a file separate from the
/// code module itself (as is the case when .pdb or .dSYM files are used),
/// this will be different from code_file.  If debugging information is
/// stored in the code module itself (possibly prior to stripping), this
/// will be the same as code_file.
///
/// The return value is an owning pointer. Release memory with string_delete.
char *code_module_debug_file(const code_module_t *module);

/// Returns a string identifying the specific version and build of the
/// associated debug file.  This may be the same as code_identifier when
/// the debug_file and code_file are identical or when the same identifier
/// is used to identify distinct debug and code files.
///
/// It usually comprises the library's UUID and an age field. On Windows, the
/// age field is a generation counter, on all other platforms it is mostly
/// zero.
///
/// The return value is an owning pointer. Release memory with string_delete.
char *code_module_debug_identifier(const code_module_t *module);

#ifdef __cplusplus
}
#endif

#endif
