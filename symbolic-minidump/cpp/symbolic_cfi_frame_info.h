#ifndef SENTRY_CFI_FRAME_INFO_H
#define SENTRY_CFI_FRAME_INFO_H

#include "processor/cfi_frame_info.h"
using google_breakpad::MemoryRegion;

class SymbolicCFIFrameInfo : public google_breakpad::CFIFrameInfo {
   public:
    SymbolicCFIFrameInfo(void *cfi_frame_info);
    ~SymbolicCFIFrameInfo();

    virtual bool FindCallerRegs(const RegisterValueMap<uint32_t>& registers,
                              const MemoryRegion& memory,
                              RegisterValueMap<uint32_t>* caller_registers) const;

    virtual bool FindCallerRegs(const RegisterValueMap<uint64_t>& registers,
                              const MemoryRegion& memory,
                              RegisterValueMap<uint64_t>* caller_registers) const;

   private:
    void *cfi_frame_info_;
};

#endif
