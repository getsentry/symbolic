#include "cpp/symbolic_cfi_frame_info.h"

#include "cpp/data_structures.h"
#include "google_breakpad/processor/minidump.h"

extern "C" {
void *evaluator_new(bool is_big_endian, void *cfi_rules, uint64_t address);
void evaluator_free(void *evaluator);
void evaluator_set_memory_region(void *evaluator,
                                 uint64_t memory_base,
                                 size_t memory_len,
                                 const void *memory_bytes);
void evaluator_set_registers(void *evaluator,
                             const regval_t *registers,
                             size_t registers_len);
regval_t *evaluator_find_caller_regs_cfi(void *evaluator,
                                         size_t *caller_registers_len_out);
void regvals_free(regval_t *reg_vals, size_t len);
}

SymbolicCFIFrameInfo::SymbolicCFIFrameInfo(bool is_big_endian,
                                           void *cfi_rules,
                                           uint64_t address) {
    evaluator_ = evaluator_new(is_big_endian, cfi_rules, address);
}

SymbolicCFIFrameInfo::~SymbolicCFIFrameInfo() {
    evaluator_free(evaluator_);
}

// Provide implementations for CFIFrameInfo to change FindCallerRegs, which
// cannot be declared virtual.
namespace google_breakpad {

template <typename V>
bool CFIFrameInfo::FindCallerRegs(const RegisterValueMap<V> &registers,
                                  const MemoryRegion &memory,
                                  RegisterValueMap<V> *caller_registers) const {
    void *evaluator =
        dynamic_cast<const SymbolicCFIFrameInfo *>(this)->evaluator_;

    caller_registers->clear();

    const MinidumpMemoryRegion *minidump_memory =
        static_cast<const MinidumpMemoryRegion *>(&memory);
    evaluator_set_memory_region(evaluator, minidump_memory->GetBase(),
                                minidump_memory->GetSize(),
                                minidump_memory->GetMemory());

    std::vector<regval_t> register_vec;
    for (typename RegisterValueMap<V>::const_iterator it = registers.begin();
         it != registers.end(); it++) {
        regval_t regval = {it->first.c_str(), it->second, sizeof(V)};
        register_vec.push_back(regval);
    }
    evaluator_set_registers(evaluator, register_vec.data(),
                            register_vec.size());

    size_t caller_registers_size = 0;
    regval_t *caller_registers_vec =
        evaluator_find_caller_regs_cfi(evaluator, &caller_registers_size);
    if (caller_registers_vec == NULL) {
        return false;
    }

    for (size_t i = 0; i < caller_registers_size; i++) {
        regval_t reg = caller_registers_vec[i];
        (*caller_registers)[reg.name] = reg.value;
    }

    regvals_free(caller_registers_vec, caller_registers_size);
    return true;
}

// Explicit instantiations for 32-bit and 64-bit architectures.
template bool CFIFrameInfo::FindCallerRegs<uint32_t>(
    const RegisterValueMap<uint32_t> &registers,
    const MemoryRegion &memory,
    RegisterValueMap<uint32_t> *caller_registers) const;
template bool CFIFrameInfo::FindCallerRegs<uint64_t>(
    const RegisterValueMap<uint64_t> &registers,
    const MemoryRegion &memory,
    RegisterValueMap<uint64_t> *caller_registers) const;

// noop implementations for unused functions

bool CFIRuleParser::Parse(const string &rule_set) {
    return false;
}
void CFIFrameInfoParseHandler::CFARule(const string &expression) {
}
void CFIFrameInfoParseHandler::RARule(const string &expression) {
}
void CFIFrameInfoParseHandler::RegisterRule(const string &name,
                                            const string &expression) {
}

}  // namespace google_breakpad
