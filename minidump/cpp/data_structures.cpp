#include <vector>

#include "google_breakpad/processor/stack_frame_cpu.h"

#include "cpp/c_string.h"
#include "cpp/data_definitions.h"
#include "cpp/data_structures.h"

using google_breakpad::StackFrame;
using google_breakpad::StackFrameAMD64;
using google_breakpad::StackFrameARM;
using google_breakpad::StackFrameARM64;
using google_breakpad::StackFramePPC;
using google_breakpad::StackFramePPC64;
using google_breakpad::StackFrameX86;

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

const code_module_t **process_state_modules(process_state_t *state,
                                            size_t *size_out) {
    if (state == nullptr) {
        return nullptr;
    }

    auto *modules = process_state_t::cast(state)->modules();
    if (modules == nullptr) {
        return nullptr;
    }

    unsigned int size = modules->module_count();
    const code_module_t **buffer = new const code_module_t *[size];
    for (unsigned int i = 0; i < size; i++) {
        buffer[i] = code_module_t::cast(modules->GetModuleAtIndex(i));
    }

    if (size_out != nullptr) {
        *size_out = size;
    }

    return buffer;
}

int32_t process_state_requesting_thread(const process_state_t *state) {
    if (state == nullptr) {
        return -1;
    }

    return process_state_t::cast(state)->requesting_thread();
}

uint64_t process_state_timestamp(const process_state_t *state) {
    if (state == nullptr) {
        return 0;
    }

    return process_state_t::cast(state)->time_date_stamp();
}

bool process_state_crashed(const process_state_t *state) {
    if (state == nullptr) {
        return false;
    }

    return process_state_t::cast(state)->crashed();
}

uint64_t process_state_crash_address(const process_state_t *state) {
    if (state == nullptr) {
        return 0;
    }

    return process_state_t::cast(state)->crash_address();
}

char *process_state_crash_reason(const process_state_t *state) {
    if (state == nullptr) {
        return nullptr;
    }

    return string_from(process_state_t::cast(state)->crash_reason());
}

char *process_state_assertion(const process_state_t *state) {
    if (state == nullptr) {
        return nullptr;
    }

    return string_from(process_state_t::cast(state)->assertion());
}

const system_info_t *process_state_system_info(const process_state_t *state) {
    if (state == nullptr) {
        return nullptr;
    }

    return system_info_t::cast(process_state_t::cast(state)->system_info());
}

char *system_info_os_name(const system_info_t *info) {
    if (info == nullptr) {
        return nullptr;
    }

    return string_from(system_info_t::cast(info)->os);
}

char *system_info_os_version(const system_info_t *info) {
    if (info == nullptr) {
        return nullptr;
    }

    return string_from(system_info_t::cast(info)->os_version);
}

char *system_info_cpu_family(const system_info_t *info) {
    if (info == nullptr) {
        return nullptr;
    }

    return string_from(system_info_t::cast(info)->cpu);
}

char *system_info_cpu_info(const system_info_t *info) {
    if (info == nullptr) {
        return nullptr;
    }

    return string_from(system_info_t::cast(info)->cpu_info);
}

uint32_t system_info_cpu_count(const system_info_t *info) {
    if (info == nullptr) {
        return 0;
    }

    return system_info_t::cast(info)->cpu_count;
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

uint64_t stack_frame_return_address(const stack_frame_t *frame) {
    if (frame == nullptr) {
        return 0;
    }

    return stack_frame_t::cast(frame)->ReturnAddress();
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

regval_t *stack_frame_registers(const stack_frame_t *frame,
                                uint32_t family,
                                size_t *size_out) {
    if (frame == nullptr) {
        return nullptr;
    }

    std::vector<regval_t> registers;

    switch (family) {
        case 1: {  // Intel32
            const StackFrameX86 *frame_x86 =
                reinterpret_cast<const StackFrameX86 *>(frame);

            if (frame_x86->context_validity & StackFrameX86::CONTEXT_VALID_EIP)
                registers.push_back({"eip", frame_x86->context.eip, 4});
            if (frame_x86->context_validity & StackFrameX86::CONTEXT_VALID_ESP)
                registers.push_back({"esp", frame_x86->context.esp, 4});
            if (frame_x86->context_validity & StackFrameX86::CONTEXT_VALID_EBP)
                registers.push_back({"ebp", frame_x86->context.ebp, 4});
            if (frame_x86->context_validity & StackFrameX86::CONTEXT_VALID_EBX)
                registers.push_back({"ebx", frame_x86->context.ebx, 4});
            if (frame_x86->context_validity & StackFrameX86::CONTEXT_VALID_ESI)
                registers.push_back({"esi", frame_x86->context.esi, 4});
            if (frame_x86->context_validity & StackFrameX86::CONTEXT_VALID_EDI)
                registers.push_back({"edi", frame_x86->context.edi, 4});
            if (frame_x86->context_validity ==
                StackFrameX86::CONTEXT_VALID_ALL) {
                registers.push_back({"eax", frame_x86->context.eax, 4});
                registers.push_back({"ecx", frame_x86->context.ecx, 4});
                registers.push_back({"edx", frame_x86->context.edx, 4});
                registers.push_back({"eflags", frame_x86->context.eflags, 4});
            }

            break;
        }

        case 2: {  // Intel64,
            const StackFrameAMD64 *frame_amd64 =
                reinterpret_cast<const StackFrameAMD64 *>(frame);

            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_RAX)
                registers.push_back({"rax", frame_amd64->context.rax, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_RDX)
                registers.push_back({"rdx", frame_amd64->context.rdx, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_RCX)
                registers.push_back({"rcx", frame_amd64->context.rcx, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_RBX)
                registers.push_back({"rbx", frame_amd64->context.rbx, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_RSI)
                registers.push_back({"rsi", frame_amd64->context.rsi, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_RDI)
                registers.push_back({"rdi", frame_amd64->context.rdi, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_RBP)
                registers.push_back({"rbp", frame_amd64->context.rbp, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_RSP)
                registers.push_back({"rsp", frame_amd64->context.rsp, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_R8)
                registers.push_back({"r8", frame_amd64->context.r8, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_R9)
                registers.push_back({"r9", frame_amd64->context.r9, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_R10)
                registers.push_back({"r10", frame_amd64->context.r10, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_R11)
                registers.push_back({"r11", frame_amd64->context.r11, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_R12)
                registers.push_back({"r12", frame_amd64->context.r12, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_R13)
                registers.push_back({"r13", frame_amd64->context.r13, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_R14)
                registers.push_back({"r14", frame_amd64->context.r14, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_R15)
                registers.push_back({"r15", frame_amd64->context.r15, 8});
            if (frame_amd64->context_validity &
                StackFrameAMD64::CONTEXT_VALID_RIP)
                registers.push_back({"rip", frame_amd64->context.rip, 8});

            break;
        }

        case 3: {  // Arm32,
            const StackFrameARM *frame_arm =
                reinterpret_cast<const StackFrameARM *>(frame);

            // Argument registers (caller-saves), which will likely only be
            // valid for the youngest frame.
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_R0)
                registers.push_back({"r0", frame_arm->context.iregs[0], 4});
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_R1)
                registers.push_back({"r1", frame_arm->context.iregs[1], 4});
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_R2)
                registers.push_back({"r2", frame_arm->context.iregs[2], 4});
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_R3)
                registers.push_back({"r3", frame_arm->context.iregs[3], 4});

            // General-purpose callee-saves registers.
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_R4)
                registers.push_back({"r4", frame_arm->context.iregs[4], 4});
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_R5)
                registers.push_back({"r5", frame_arm->context.iregs[5], 4});
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_R6)
                registers.push_back({"r6", frame_arm->context.iregs[6], 4});
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_R7)
                registers.push_back({"r7", frame_arm->context.iregs[7], 4});
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_R8)
                registers.push_back({"r8", frame_arm->context.iregs[8], 4});
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_R9)
                registers.push_back({"r9", frame_arm->context.iregs[9], 4});
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_R10)
                registers.push_back({"r10", frame_arm->context.iregs[10], 4});
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_R12)
                registers.push_back({"r12", frame_arm->context.iregs[12], 4});

            // Registers with a dedicated or conventional purpose.
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_FP)
                registers.push_back({"fp", frame_arm->context.iregs[11], 4});
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_SP)
                registers.push_back({"sp", frame_arm->context.iregs[13], 4});
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_LR)
                registers.push_back({"lr", frame_arm->context.iregs[14], 4});
            if (frame_arm->context_validity & StackFrameARM::CONTEXT_VALID_PC)
                registers.push_back({"pc", frame_arm->context.iregs[15], 4});

            break;
        }

        case 4: {  // Arm64,
            const StackFrameARM64 *frame_arm64 =
                reinterpret_cast<const StackFrameARM64 *>(frame);

            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X0)
                registers.push_back({"x0", frame_arm64->context.iregs[0], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X1)
                registers.push_back({"x1", frame_arm64->context.iregs[1], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X2)
                registers.push_back({"x2", frame_arm64->context.iregs[2], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X3)
                registers.push_back({"x3", frame_arm64->context.iregs[3], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X4)
                registers.push_back({"x4", frame_arm64->context.iregs[4], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X5)
                registers.push_back({"x5", frame_arm64->context.iregs[5], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X6)
                registers.push_back({"x6", frame_arm64->context.iregs[6], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X7)
                registers.push_back({"x7", frame_arm64->context.iregs[7], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X8)
                registers.push_back({"x8", frame_arm64->context.iregs[8], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X9)
                registers.push_back({"x9", frame_arm64->context.iregs[9], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X10)
                registers.push_back({"x10", frame_arm64->context.iregs[10], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X11)
                registers.push_back({"x11", frame_arm64->context.iregs[11], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X12)
                registers.push_back({"x12", frame_arm64->context.iregs[12], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X13)
                registers.push_back({"x13", frame_arm64->context.iregs[13], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X14)
                registers.push_back({"x14", frame_arm64->context.iregs[14], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X15)
                registers.push_back({"x15", frame_arm64->context.iregs[15], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X16)
                registers.push_back({"x16", frame_arm64->context.iregs[16], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X17)
                registers.push_back({"x17", frame_arm64->context.iregs[17], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X18)
                registers.push_back({"x18", frame_arm64->context.iregs[18], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X19)
                registers.push_back({"x19", frame_arm64->context.iregs[19], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X20)
                registers.push_back({"x20", frame_arm64->context.iregs[20], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X21)
                registers.push_back({"x21", frame_arm64->context.iregs[21], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X22)
                registers.push_back({"x22", frame_arm64->context.iregs[22], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X23)
                registers.push_back({"x23", frame_arm64->context.iregs[23], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X24)
                registers.push_back({"x24", frame_arm64->context.iregs[24], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X25)
                registers.push_back({"x25", frame_arm64->context.iregs[25], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X26)
                registers.push_back({"x26", frame_arm64->context.iregs[26], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X27)
                registers.push_back({"x27", frame_arm64->context.iregs[27], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_X28)
                registers.push_back({"x28", frame_arm64->context.iregs[28], 8});

            // Registers with a dedicated or conventional purpose.
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_FP)
                registers.push_back({"x29", frame_arm64->context.iregs[29], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_LR)
                registers.push_back({"x30", frame_arm64->context.iregs[30], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_SP)
                registers.push_back({"sp", frame_arm64->context.iregs[31], 8});
            if (frame_arm64->context_validity &
                StackFrameARM64::CONTEXT_VALID_PC)
                registers.push_back({"pc", frame_arm64->context.iregs[32], 8});

            break;
        }

        case 5: {  // Ppc32,
            const StackFramePPC *frame_ppc =
                reinterpret_cast<const StackFramePPC *>(frame);

            if (frame_ppc->context_validity & StackFramePPC::CONTEXT_VALID_SRR0)
                registers.push_back({"srr0", frame_ppc->context.srr0, 4});
            if (frame_ppc->context_validity & StackFramePPC::CONTEXT_VALID_GPR1)
                registers.push_back({"r1", frame_ppc->context.gpr[1], 4});

            break;
        }

        case 6: {  // Ppc64,
            const StackFramePPC64 *frame_ppc =
                reinterpret_cast<const StackFramePPC64 *>(frame);

            if (frame_ppc->context_validity &
                StackFramePPC64::CONTEXT_VALID_SRR0)
                registers.push_back({"srr0", frame_ppc->context.srr0, 8});
            if (frame_ppc->context_validity &
                StackFramePPC64::CONTEXT_VALID_GPR1)
                registers.push_back({"r1", frame_ppc->context.gpr[1], 8});

            break;
        }

        case 0:  // Unknown
        default:
            break;  // leave registers empty
    }

    regval_t *buffer = new regval_t[registers.size()];
    std::copy(registers.begin(), registers.end(), buffer);
    if (size_out != nullptr) {
        *size_out = registers.size();
    }

    return buffer;
}

void regval_delete(regval_t *regval) {
    if (regval != nullptr) {
        delete[] regval;
    }
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

void code_modules_delete(code_module_t **modules) {
    if (modules != nullptr) {
        delete[] modules;
    }
}
