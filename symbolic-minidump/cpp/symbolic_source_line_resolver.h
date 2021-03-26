#ifndef symbolic_source_line_resolver_h_INCLUDED
#define symbolic_source_line_resolver_h_INCLUDED

#include "google_breakpad/processor/source_line_resolver_interface.h"
#include "google_breakpad/processor/stack_frame.h"
#include "processor/cfi_frame_info.h"
#include "processor/windows_frame_info.h"

using namespace google_breakpad;

struct Evaluator {};

class SymbolicSourceLineResolver
    : public google_breakpad::SourceLineResolverInterface {
   public:
    SymbolicSourceLineResolver(void *resolver, bool is_big_endian);
    virtual ~SymbolicSourceLineResolver() {
    }

    bool HasModule(const CodeModule *module);
    CFIFrameInfo *FindCFIFrameInfo(const StackFrame *frame);

   private:
    void *resolver_;
    // Disallow unwanted copy ctor and assignment operator
    SymbolicSourceLineResolver(const SymbolicSourceLineResolver &);
    void operator=(const SymbolicSourceLineResolver &);
};

#endif
