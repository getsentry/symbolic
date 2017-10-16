#ifndef SENTRY_MMAP_SYMBOL_SUPPLIER_H
#define SENTRY_MMAP_SYMBOL_SUPPLIER_H

#include <map>

#include "cpp/processor.h"
#include "google_breakpad/processor/symbol_supplier.h"

class MmapSymbolSupplier : public google_breakpad::SymbolSupplier {
 public:
  explicit MmapSymbolSupplier(size_t symbol_count,
                              const symbol_entry_t *symbols);

  virtual ~MmapSymbolSupplier() {
  }

  virtual SymbolResult GetSymbolFile(
      const google_breakpad::CodeModule *module,
      const google_breakpad::SystemInfo *system_info,
      string *symbol_file);

  virtual SymbolResult GetSymbolFile(
      const google_breakpad::CodeModule *module,
      const google_breakpad::SystemInfo *system_info,
      string *symbol_file,
      string *symbol_data);

  virtual SymbolResult GetCStringSymbolData(
      const google_breakpad::CodeModule *module,
      const google_breakpad::SystemInfo *system_info,
      string *symbol_file,
      char **symbol_data,
      size_t *symbol_data_size);

  virtual void FreeSymbolData(const google_breakpad::CodeModule *module);

 private:
  std::map<std::string, const symbol_entry_t *> cache;
};

#endif
