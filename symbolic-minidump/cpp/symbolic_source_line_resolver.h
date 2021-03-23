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
    : public google_breakpad::SourceLineResolverBase {
   public:
    SymbolicSourceLineResolver(bool is_big_endian);
    virtual ~SymbolicSourceLineResolver() {
    }

    using SourceLineResolverBase::FillSourceLineInfo;
    using SourceLineResolverBase::FindCFIFrameInfo;
    using SourceLineResolverBase::FindWindowsFrameInfo;
    using SourceLineResolverBase::HasModule;
    using SourceLineResolverBase::IsModuleCorrupt;
    using SourceLineResolverBase::LoadModule;
    using SourceLineResolverBase::LoadModuleUsingMapBuffer;
    using SourceLineResolverBase::LoadModuleUsingMemoryBuffer;
    using SourceLineResolverBase::ShouldDeleteMemoryBufferAfterLoadModule;
    using SourceLineResolverBase::UnloadModule;

   private:
    // friend declarations:
    friend class SymbolicModuleFactory;
    friend class ModuleComparer;
    friend class ModuleSerializer;
    template <class>
    friend class SimpleSerializer;

    // Function derives from SourceLineResolverBase::Function.
    struct Function;
    // Module implements SourceLineResolverBase::Module interface.
    class Module;

    // Disallow unwanted copy ctor and assignment operator
    SymbolicSourceLineResolver(const SymbolicSourceLineResolver &);
    void operator=(const SymbolicSourceLineResolver &);
};

struct SymbolicSourceLineResolver::Function
    : public SourceLineResolverBase::Function {
    Function(const string &function_name,
             MemAddr function_address,
             MemAddr code_size,
             int set_parameter_size,
             bool is_mutiple)
        : Base(function_name,
               function_address,
               code_size,
               set_parameter_size,
               is_mutiple),
          lines() {
    }
    RangeMap<MemAddr, linked_ptr<Line> > lines;

   private:
    typedef SourceLineResolverBase::Function Base;
};

class SymbolicSourceLineResolver::Module
    : public SourceLineResolverBase::Module {
   public:
    Module(const string &name, bool is_big_endian);
    virtual ~Module();

    // Loads a map from the given buffer in char* type.
    // Does NOT have ownership of memory_buffer.
    // The passed in |memory buffer| is of size |memory_buffer_size|.  If it is
    // not null terminated, LoadMapFromMemory() will null terminate it by
    // modifying the passed in buffer.
    virtual bool LoadMapFromMemory(char *memory_buffer,
                                   size_t memory_buffer_size);

    // Tells whether the loaded symbol data is corrupt.  Return value is
    // undefined, if the symbol data hasn't been loaded yet.
    virtual bool IsCorrupt() const {
        return is_corrupt_;
    }

    // Looks up the given relative address, and fills the StackFrame struct
    // with the result.
    virtual void LookupAddress(StackFrame *frame) const;

    // If Windows stack walking information is available covering ADDRESS,
    // return a WindowsFrameInfo structure describing it. If the information
    // is not available, returns NULL. A NULL return value does not indicate
    // an error. The caller takes ownership of any returned WindowsFrameInfo
    // object.
    virtual WindowsFrameInfo *FindWindowsFrameInfo(
        const StackFrame *frame) const;

    // If CFI stack walking information is available covering ADDRESS,
    // return a CFIFrameInfo structure describing it. If the information
    // is not available, return NULL. The caller takes ownership of any
    // returned CFIFrameInfo object.
    virtual CFIFrameInfo *FindCFIFrameInfo(const StackFrame *frame) const;

   private:
    // Friend declarations.
    friend class SymbolicSourceLineResolver;
    friend class ModuleComparer;
    friend class ModuleSerializer;

    typedef std::map<int, string> FileMap;

    // Logs parse errors.  |*num_errors| is increased every time LogParseError
    // is called.
    static void LogParseError(const string &message,
                              int line_number,
                              int *num_errors);

    // Parses a file declaration
    bool ParseFile(char *file_line);

    // Parses a function declaration, returning a new Function object.
    Function *ParseFunction(char *function_line);

    // Parses a line declaration, returning a new Line object.
    Line *ParseLine(char *line_line);

    // Parses a PUBLIC symbol declaration, storing it in public_symbols_.
    // Returns false if an error occurs.
    bool ParsePublicSymbol(char *public_line);

    // Parses a STACK WIN or STACK CFI frame info declaration, storing
    // it in the appropriate table.
    bool ParseStackInfo(char *stack_info_line);

    // Parses a STACK CFI record, storing it in cfi_frame_info_.
    bool ParseCFIFrameInfo(char *stack_info_line);

    string name_;
    FileMap files_;
    RangeMap<MemAddr, linked_ptr<Function> > functions_;
    AddressMap<MemAddr, linked_ptr<PublicSymbol> > public_symbols_;
    bool is_corrupt_;

    // Each element in the array is a ContainedRangeMap for a type
    // listed in WindowsFrameInfoTypes. These are split by type because
    // there may be overlaps between maps of different types, but some
    // information is only available as certain types.
    ContainedRangeMap<MemAddr, linked_ptr<WindowsFrameInfo> >
        windows_frame_info_[WindowsFrameInfo::STACK_INFO_LAST];

    void *cfi_rules_;
    bool is_big_endian_;
};

class SymbolicModuleFactory : public google_breakpad::ModuleFactory {
   public:
    SymbolicModuleFactory(bool is_big_endian) {
        is_big_endian_ = is_big_endian;
    }
    virtual ~SymbolicModuleFactory() {
    }
    virtual SymbolicSourceLineResolver::Module *CreateModule(
        const string &name) const {
        return new SymbolicSourceLineResolver::Module(name, is_big_endian_);
    }

   private:
    bool is_big_endian_;
};
#endif  // symbolic_source_line_resolver_h_INCLUDED
