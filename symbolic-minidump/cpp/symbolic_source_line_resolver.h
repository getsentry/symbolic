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
    WindowsFrameInfo *FindWindowsFrameInfo(const StackFrame *frame);

    bool LoadModule(const CodeModule *module, const string &map_file) {
        return false;
    }

    bool LoadModuleUsingMapBuffer(const CodeModule *module,
                                  const string &map_buffer) {
        return false;
    }

    bool LoadModuleUsingMemoryBuffer(const CodeModule *module,
                                     char *memory_buffer,
                                     size_t memory_buffer_size) {
        return false;
    }

    bool ShouldDeleteMemoryBufferAfterLoadModule() {
        return false;
    }

    void UnloadModule(const CodeModule *module) {
    }

    bool IsModuleCorrupt(const CodeModule *module) {
        return false;
    }

    void FillSourceLineInfo(StackFrame *frame) {
    }

   private:
    void *resolver_;
    std::vector<WindowsFrameInfo> windows_frame_infos_;
    // Disallow unwanted copy ctor and assignment operator
    SymbolicSourceLineResolver(const SymbolicSourceLineResolver &);
    void operator=(const SymbolicSourceLineResolver &);
};

#endif
