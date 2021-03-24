#ifndef SENTRY_CFI_FRAME_INFO_H
#define SENTRY_CFI_FRAME_INFO_H

#include "processor/cfi_frame_info.h"

class SymbolicCFIFrameInfo : public google_breakpad::CFIFrameInfo {
   public:
    SymbolicCFIFrameInfo(void *evaluator);
    ~SymbolicCFIFrameInfo();

   private:
    void *evaluator_;

    friend class google_breakpad::CFIFrameInfo;
};

#endif
