#include <string>

#include "cpp/mmap_symbol_supplier.h"
#include "google_breakpad/processor/code_module.h"

using google_breakpad::CodeModule;
using google_breakpad::SystemInfo;

MmapSymbolSupplier::MmapSymbolSupplier(size_t symbol_count,
                                       const symbol_entry_t *symbols) {
  for (const symbol_entry_t *entry = symbols; entry < symbols + symbol_count;
       ++entry) {
    cache[entry->debug_identifier] = entry;
  }
}

MmapSymbolSupplier::SymbolResult MmapSymbolSupplier::GetSymbolFile(
    const CodeModule *module,
    const SystemInfo *system_info,
    string *symbol_file) {
  string symbol_data;
  return GetSymbolFile(module, system_info, symbol_file, &symbol_data);
}

MmapSymbolSupplier::SymbolResult MmapSymbolSupplier::GetSymbolFile(
    const CodeModule *module,
    const SystemInfo *system_info,
    string *symbol_file,
    string *symbol_data) {
  char *raw_data;
  size_t data_size;

  SymbolResult result = GetCStringSymbolData(module, system_info, symbol_file,
                                             &raw_data, &data_size);

  if (result == FOUND) {
    symbol_data->assign(raw_data, data_size);
  }

  return result;
}

MmapSymbolSupplier::SymbolResult MmapSymbolSupplier::GetCStringSymbolData(
    const CodeModule *module,
    const SystemInfo *system_info,
    string *symbol_file,
    char **symbol_data,
    size_t *symbol_size) {
  auto it = cache.find(module->debug_identifier());
  if (it == cache.end()) {
    return NOT_FOUND;
  }

  *symbol_file = it->first;
  *symbol_size = it->second->symbol_size;
  *symbol_data = const_cast<char *>(it->second->symbol_data);

  return FOUND;
}

void MmapSymbolSupplier::FreeSymbolData(const CodeModule *module) {
  // Nothing to do. Managed by the owner of the supplier
}
