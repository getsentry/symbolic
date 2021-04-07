#ifndef SENTRY_CFI_FRAME_INFO_H
#define SENTRY_CFI_FRAME_INFO_H

#include "processor/cfi_frame_info.h"

class SymbolicCFIFrameInfo : public google_breakpad::CFIFrameInfo {
   public:
    SymbolicCFIFrameInfo(void *cfi_frame_info);
    ~SymbolicCFIFrameInfo();

   private:
    void *cfi_frame_info_;

    friend class google_breakpad::CFIFrameInfo;
};

#endif
