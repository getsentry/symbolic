/* c bindings to the symbolic library */

#ifndef SYMBOLIC_H_INCLUDED
#define SYMBOLIC_H_INCLUDED

#include <stdint.h>
#include <stdlib.h>
#include <stdbool.h>

/*
 * Indicates the error that ocurred
 */
enum SymbolicErrorCode {
  SYMBOLIC_ERROR_CODE_NO_ERROR = 0,
  SYMBOLIC_ERROR_CODE_PANIC = 1,
  SYMBOLIC_ERROR_CODE_INTERNAL = 2,
  SYMBOLIC_ERROR_CODE_MSG = 3,
  SYMBOLIC_ERROR_CODE_UNKNOWN = 4,
  SYMBOLIC_ERROR_CODE_PARSE = 101,
  SYMBOLIC_ERROR_CODE_NOT_FOUND = 102,
  SYMBOLIC_ERROR_CODE_FORMAT = 103,
  SYMBOLIC_ERROR_CODE_BAD_SYMBOL = 1001,
  SYMBOLIC_ERROR_CODE_UNSUPPORTED_OBJECT_FILE = 1002,
  SYMBOLIC_ERROR_CODE_MALFORMED_OBJECT_FILE = 1003,
  SYMBOLIC_ERROR_CODE_BAD_CACHE_FILE = 1004,
  SYMBOLIC_ERROR_CODE_MISSING_SECTION = 1005,
  SYMBOLIC_ERROR_CODE_BAD_DWARF_DATA = 1006,
  SYMBOLIC_ERROR_CODE_MISSING_DEBUG_INFO = 1007,
  SYMBOLIC_ERROR_CODE_IO = 10001,
  SYMBOLIC_ERROR_CODE_UTF8_ERROR = 10002,
};
typedef uint32_t SymbolicErrorCode;

/*
 * A potential multi arch object.
 */
struct SymbolicFatObject;
typedef struct SymbolicFatObject SymbolicFatObject;

/*
 * A single arch object.
 */
struct SymbolicObject;
typedef struct SymbolicObject SymbolicObject;

/*
 * Represents a symbolic sym cache.
 */
struct SymbolicSymCache;
typedef struct SymbolicSymCache SymbolicSymCache;

/*
 * Represents a string.
 */
typedef struct {
  char *data;
  size_t len;
  bool owned;
} SymbolicStr;

typedef struct {
  uint32_t cputype;
  uint32_t cpusubtype;
} SymbolicMachoArch;

/*
 * Represents an instruction info.
 */
typedef struct {
  /*
   * The address of the instruction we want to use as a base.
   */
  uint64_t addr;
  /*
   * The architecture we are dealing with.
   */
  const SymbolicStr *arch;
  /*
   * This is true if the frame is the cause of the crash.
   */
  bool crashing_frame;
  /*
   * If a signal is know that triggers the crash, it can be stored here (0 if unknown)
   */
  uint32_t signal;
  /*
   * The optional value of the IP register (0 if unknown).
   */
  uint64_t ip_reg;
} SymbolicInstructionInfo;

/*
 * Represents a single symbol after lookup.
 */
typedef struct {
  uint64_t sym_addr;
  uint64_t instr_addr;
  uint32_t line;
  SymbolicStr symbol;
  SymbolicStr filename;
  SymbolicStr base_dir;
  SymbolicStr comp_dir;
} SymbolicSymbol;

/*
 * Represents a lookup result of one or more items.
 */
typedef struct {
  SymbolicSymbol *items;
  size_t len;
} SymbolicLookupResult;

/*
 * Represents a UUID
 */
typedef struct {
  uint8_t data[16];
} SymbolicUuid;

/*
 * Checks if an architecture is known.
 */
SymbolicStr symbolic_arch_from_macho(const SymbolicMachoArch *arch);

/*
 * Returns the name of the instruction pointer if known.
 */
SymbolicStr symbolic_arch_ip_reg_name(const SymbolicStr *arch);

/*
 * Checks if an architecture is known.
 */
bool symbolic_arch_is_known(const SymbolicStr *arch);

/*
 * Returns the macho code for a CPU architecture.
 */
SymbolicMachoArch symbolic_arch_to_macho(const SymbolicStr *arch);

/*
 * Demangles a given identifier.
 * 
 * This demangles with the default behavior in symbolic.
 */
SymbolicStr symbolic_demangle(const SymbolicStr *ident);

/*
 * Demangles a given identifier.
 * 
 * This is similar to `symbolic_demangle` but does not demangle the
 * arguments and instead strips them.
 */
SymbolicStr symbolic_demangle_no_args(const SymbolicStr *ident);

/*
 * Clears the last error.
 */
void symbolic_err_clear();

/*
 * Returns the last error code.
 * 
 * If there is no error, 0 is returned.
 */
SymbolicErrorCode symbolic_err_get_last_code();

/*
 * Returns the last error message.
 * 
 * If there is no error an empty string is returned.  This allocates new memory
 * that needs to be freed with `symbolic_str_free`.
 */
SymbolicStr symbolic_err_get_last_message();

/*
 * Returns the panic information as string.
 */
SymbolicStr symbolic_err_get_panic_info();

/*
 * Frees the given fat object.
 */
void symbolic_fatobject_free(SymbolicFatObject *sfo);

/*
 * Returns the n-th object.
 */
SymbolicObject *symbolic_fatobject_get_object(const SymbolicFatObject *sfo, size_t idx);

/*
 * Returns the number of contained objects.
 */
size_t symbolic_fatobject_object_count(const SymbolicFatObject *sfo);

/*
 * Loads a fat object from a given path.
 */
SymbolicFatObject *symbolic_fatobject_open(const char *path);

/*
 * Return the best instruction for an isntruction info
 */
uint64_t symbolic_find_best_instruction(const SymbolicInstructionInfo *ii);

/*
 * Initializes the library
 */
void symbolic_init();

/*
 * Frees a lookup result.
 */
void symbolic_lookup_result_free(SymbolicLookupResult *slr);

/*
 * Frees an object returned from a fat object.
 */
void symbolic_object_free(SymbolicObject *so);

/*
 * Returns the architecture of the object.
 */
SymbolicStr symbolic_object_get_arch(const SymbolicObject *so);

/*
 * Returns the object kind
 */
SymbolicStr symbolic_object_get_kind(const SymbolicObject *so);

/*
 * Returns the UUID of an object.
 */
SymbolicUuid symbolic_object_get_uuid(const SymbolicObject *so);

/*
 * Frees a symbolic str.
 * 
 * If the string is marked as not owned then this function does not
 * do anything.
 */
void symbolic_str_free(SymbolicStr *s);

/*
 * Creates a symbolic str from a c string.
 * 
 * This sets the string to owned.  In case it's not owned you either have
 * to make sure you are not freeing the memory or you need to set the
 * owned flag to false.
 */
SymbolicStr symbolic_str_from_cstr(const char *s);

/*
 * Returns the version of the cache file.
 */
uint32_t symbolic_symcache_file_format_version(const SymbolicSymCache *scache);

/*
 * Frees a symcache object.
 */
void symbolic_symcache_free(SymbolicSymCache *scache);

/*
 * Creates a symcache from bytes
 */
SymbolicSymCache *symbolic_symcache_from_bytes(const uint8_t *bytes, size_t len);

/*
 * Creates a symcache from a given object.
 */
SymbolicSymCache *symbolic_symcache_from_object(const SymbolicObject *sobj);

/*
 * Creates a symcache from a given path.
 */
SymbolicSymCache *symbolic_symcache_from_path(const char *path);

/*
 * Returns the architecture of the symcache.
 */
SymbolicStr symbolic_symcache_get_arch(const SymbolicSymCache *scache);

/*
 * Returns the internal buffer of the symcache.
 * 
 * The internal buffer is exactly `symbolic_symcache_get_size` bytes long.
 */
const uint8_t *symbolic_symcache_get_bytes(const SymbolicSymCache *scache);

/*
 * Returns the size in bytes of the symcache.
 */
size_t symbolic_symcache_get_size(const SymbolicSymCache *scache);

/*
 * Returns the architecture of the symcache.
 */
SymbolicUuid symbolic_symcache_get_uuid(const SymbolicSymCache *scache);

/*
 * Returns true if the symcache has file infos.
 */
bool symbolic_symcache_has_file_info(const SymbolicSymCache *scache);

/*
 * Returns true if the symcache has line infos.
 */
bool symbolic_symcache_has_line_info(const SymbolicSymCache *scache);

/*
 * Returns the version of the cache file.
 */
uint32_t symbolic_symcache_latest_file_format_version();

/*
 * Looks up a single symbol.
 */
SymbolicLookupResult symbolic_symcache_lookup(const SymbolicSymCache *scache, uint64_t addr);

/*
 * Returns true if the uuid is nil
 */
bool symbolic_uuid_is_nil(const SymbolicUuid *uuid);

/*
 * Formats the UUID into a string.
 * 
 * The string is newly allocated and needs to be released with
 * `symbolic_cstr_free`.
 */
SymbolicStr symbolic_uuid_to_str(const SymbolicUuid *uuid);

#endif /* SYMBOLIC_H_INCLUDED */
