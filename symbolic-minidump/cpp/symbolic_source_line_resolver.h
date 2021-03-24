#ifndef symbolic_source_line_resolver_h_INCLUDED
#define symbolic_source_line_resolver_h_INCLUDED

#include "google_breakpad/processor/source_line_resolver_base.h"
#include "google_breakpad/processor/stack_frame.h"
#include "processor/address_map-inl.h"
#include "processor/cfi_frame_info.h"
#include "processor/contained_range_map-inl.h"
#include "processor/linked_ptr.h"
#include "processor/module_factory.h"
#include "processor/range_map-inl.h"
#include "processor/source_line_resolver_base_types.h"
#include "processor/windows_frame_info.h"

using namespace google_breakpad;

struct Evaluator {};

class SymbolicSourceLineResolver
    : public google_breakpad::SourceLineResolverInterface {
   public:
    SymbolicSourceLineResolver(){};
    virtual ~SymbolicSourceLineResolver() {
    }

    bool HasModule(const CodeModule *module);
    void FillSourceLineInfo(StackFrame *frame);
    CFIFrameInfo *FindCFIFrameInfo(const StackFrame *frame);

   private:
    // Disallow unwanted copy ctor and assignment operator
    SymbolicSourceLineResolver(const SymbolicSourceLineResolver &);
    void operator=(const SymbolicSourceLineResolver &);
};

#endif
