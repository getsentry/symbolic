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
  SYMBOLIC_ERROR_CODE_MISSING_DEBUG_INFO = 104,
  SYMBOLIC_ERROR_CODE_BAD_JSON = 105,
  SYMBOLIC_ERROR_CODE_BAD_SYMBOL = 1001,
  SYMBOLIC_ERROR_CODE_UNSUPPORTED_OBJECT_FILE = 1002,
  SYMBOLIC_ERROR_CODE_MALFORMED_OBJECT_FILE = 1003,
  SYMBOLIC_ERROR_CODE_BAD_CACHE_FILE = 1004,
  SYMBOLIC_ERROR_CODE_MISSING_SECTION = 1005,
  SYMBOLIC_ERROR_CODE_BAD_DWARF_DATA = 1006,
  SYMBOLIC_ERROR_CODE_BAD_BREAKPAD_SYM = 1007,
  SYMBOLIC_ERROR_CODE_BAD_SYMBOL_TABLE = 1008,
  SYMBOLIC_ERROR_CODE_BAD_SOURCEMAP = 2001,
  SYMBOLIC_ERROR_CODE_CANNOT_FLATTEN_SOURCEMAP = 2002,
  SYMBOLIC_ERROR_CODE_STACKWALK = 3001,
  SYMBOLIC_ERROR_CODE_RESOLVER = 3002,
  SYMBOLIC_ERROR_CODE_IO = 10001,
  SYMBOLIC_ERROR_CODE_UTF8_ERROR = 10002,
  SYMBOLIC_ERROR_CODE_PARSE_INT = 10003,
};
typedef uint32_t SymbolicErrorCode;

/*
 * Indicates how well the instruction pointer derived during stack walking is trusted
 */
enum SymbolicFrameTrust {
  SYMBOLIC_FRAME_TRUST_NONE,
  SYMBOLIC_FRAME_TRUST_SCAN,
  SYMBOLIC_FRAME_TRUST_CFI_SCAN,
  SYMBOLIC_FRAME_TRUST_FP,
  SYMBOLIC_FRAME_TRUST_CFI,
  SYMBOLIC_FRAME_TRUST_PREWALKED,
  SYMBOLIC_FRAME_TRUST_CONTEXT,
};
typedef uint32_t SymbolicFrameTrust;

/*
 * A potential multi arch object.
 */
typedef struct SymbolicFatObject SymbolicFatObject;

/*
 * Contains stack frame information (CFI) for images
 */
typedef struct SymbolicFrameInfoMap SymbolicFrameInfoMap;

/*
 * A single arch object.
 */
typedef struct SymbolicObject SymbolicObject;

/*
 * Represents a proguard mapping view
 */
typedef struct SymbolicProguardMappingView SymbolicProguardMappingView;

/*
 * Represents a sourcemap view
 */
typedef struct SymbolicSourceMapView SymbolicSourceMapView;

/*
 * Represents a source view
 */
typedef struct SymbolicSourceView SymbolicSourceView;

/*
 * Represents a symbolic sym cache.
 */
typedef struct SymbolicSymCache SymbolicSymCache;

/*
 * Represents a string.
 */
typedef struct {
  char *data;
  uintptr_t len;
  bool owned;
} SymbolicStr;

/*
 * ELF architecture
 */
typedef struct {
  uint16_t machine;
} SymbolicElfArch;

/*
 * Mach-O architecture
 */
typedef struct {
  uint32_t cputype;
  uint32_t cpusubtype;
} SymbolicMachoArch;

typedef struct {
  uint8_t *bytes;
  uintptr_t len;
} SymbolicCfiCache;

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
  uint64_t line_addr;
  uint64_t instr_addr;
  uint32_t line;
  SymbolicStr lang;
  SymbolicStr symbol;
  SymbolicStr filename;
  SymbolicStr base_dir;
  SymbolicStr comp_dir;
} SymbolicLineInfo;

/*
 * Represents a lookup result of one or more items.
 */
typedef struct {
  SymbolicLineInfo *items;
  uintptr_t len;
} SymbolicLookupResult;

/*
 * OS and CPU information
 */
typedef struct {
  SymbolicStr os_name;
  SymbolicStr os_version;
  SymbolicStr os_build;
  SymbolicStr cpu_family;
  SymbolicStr cpu_info;
  uint32_t cpu_count;
} SymbolicSystemInfo;

/*
 * Carries information about a code module loaded into the process during the crash
 */
typedef struct {
  SymbolicStr id;
  uint64_t addr;
  uint64_t size;
  SymbolicStr name;
} SymbolicCodeModule;

/*
 * Contains the absolute instruction address and image information of a stack frame
 */
typedef struct {
  uint64_t return_address;
  uint64_t instruction;
  SymbolicFrameTrust trust;
  SymbolicCodeModule module;
} SymbolicStackFrame;

/*
 * Represents a thread of the process state which holds a list of stack frames
 */
typedef struct {
  uint32_t thread_id;
  SymbolicStackFrame *frames;
  uintptr_t frame_count;
} SymbolicCallStack;

/*
 * State of a crashed process
 */
typedef struct {
  int32_t requesting_thread;
  uint64_t timestamp;
  bool crashed;
  uint64_t crash_address;
  SymbolicStr crash_reason;
  SymbolicStr assertion;
  SymbolicSystemInfo system_info;
  SymbolicCallStack *threads;
  uintptr_t thread_count;
  SymbolicCodeModule *modules;
  uintptr_t module_count;
} SymbolicProcessState;

/*
 * Represents a UUID
 */
typedef struct {
  uint8_t data[16];
} SymbolicUuid;

/*
 * Represents a single token after lookup.
 */
typedef struct {
  uint32_t src_line;
  uint32_t src_col;
  uint32_t dst_line;
  uint32_t dst_col;
  uint32_t src_id;
  SymbolicStr name;
  SymbolicStr src;
  SymbolicStr function_name;
} SymbolicTokenMatch;

/*
 * Parses a Breakpad architecture.
 */
SymbolicStr symbolic_arch_from_breakpad(const SymbolicStr *arch);

/*
 * Parses an ELF architecture.
 */
SymbolicStr symbolic_arch_from_elf(const SymbolicElfArch *arch);

/*
 * Parses a Mach-O architecture.
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
 * Returns the breakpad name for an architecture.
 */
SymbolicStr symbolic_arch_to_breakpad(const SymbolicStr *arch);

/*
 * Returns the macho code for an architecture.
 */
SymbolicMachoArch symbolic_arch_to_macho(const SymbolicStr *arch);

void symbolic_cfi_cache_free(SymbolicCfiCache *scache);

SymbolicCfiCache *symbolic_cfi_cache_from_object(const SymbolicObject *sobj);

/*
 * Demangles a given identifier.
 *
 * This demangles with the default behavior in symbolic. If no language
 * is specified, it will be auto-detected.
 */
SymbolicStr symbolic_demangle(const SymbolicStr *ident, const SymbolicStr *lang);

/*
 * Demangles a given identifier.
 *
 * This is similar to `symbolic_demangle` but does not demangle the
 * arguments and instead strips them. If no language is specified, it
 * will be auto-detected.
 */
SymbolicStr symbolic_demangle_no_args(const SymbolicStr *ident, const SymbolicStr *lang);

/*
 * Clears the last error.
 */
void symbolic_err_clear(void);

/*
 * Returns the panic information as string.
 */
SymbolicStr symbolic_err_get_backtrace(void);

/*
 * Returns the last error code.
 *
 * If there is no error, 0 is returned.
 */
SymbolicErrorCode symbolic_err_get_last_code(void);

/*
 * Returns the last error message.
 *
 * If there is no error an empty string is returned.  This allocates new memory
 * that needs to be freed with `symbolic_str_free`.
 */
SymbolicStr symbolic_err_get_last_message(void);

/*
 * Frees the given fat object.
 */
void symbolic_fatobject_free(SymbolicFatObject *sfo);

/*
 * Returns the n-th object.
 */
SymbolicObject *symbolic_fatobject_get_object(const SymbolicFatObject *sfo, uintptr_t idx);

/*
 * Returns the number of contained objects.
 */
uintptr_t symbolic_fatobject_object_count(const SymbolicFatObject *sfo);

/*
 * Loads a fat object from a given path.
 */
SymbolicFatObject *symbolic_fatobject_open(const char *path);

/*
 * Return the best instruction for an isntruction info
 */
uint64_t symbolic_find_best_instruction(const SymbolicInstructionInfo *ii);

/*
 * Adds CFI for a code module specified by the `sid` argument
 */
void symbolic_frame_info_map_add(const SymbolicFrameInfoMap *smap,
                                 const SymbolicStr *sid,
                                 const char *path);

/*
 * Frees a frame info map object
 */
void symbolic_frame_info_map_free(SymbolicFrameInfoMap *smap);

/*
 * Creates a new frame info map
 */
SymbolicFrameInfoMap *symbolic_frame_info_map_new(void);

/*
 * Converts a Breakpad CodeModuleId to DebugId.
 */
SymbolicStr symbolic_id_from_breakpad(const SymbolicStr *sid);

/*
 * Initializes the library
 */
void symbolic_init(void);

/*
 * Frees a lookup result.
 */
void symbolic_lookup_result_free(SymbolicLookupResult *slr);

/*
 * Normalizes a debug identifier to default representation.
 */
SymbolicStr symbolic_normalize_debug_id(const SymbolicStr *sid);

/*
 * Frees an object returned from a fat object.
 */
void symbolic_object_free(SymbolicObject *so);

/*
 * Returns the architecture of the object.
 */
SymbolicStr symbolic_object_get_arch(const SymbolicObject *so);

/*
 * Returns the object class
 */
SymbolicStr symbolic_object_get_debug_kind(const SymbolicObject *so);

SymbolicStr symbolic_object_get_id(const SymbolicObject *so);

/*
 * Returns the object kind
 */
SymbolicStr symbolic_object_get_kind(const SymbolicObject *so);

/*
 * Returns the object type
 */
SymbolicStr symbolic_object_get_type(const SymbolicObject *so);

/*
 * Processes a minidump with optional CFI information and returns the state
 * of the process at the time of the crash
 */
SymbolicProcessState *symbolic_process_minidump(const char *path, const SymbolicFrameInfoMap *smap);

/*
 * Processes a minidump with optional CFI information and returns the state
 * of the process at the time of the crash
 */
SymbolicProcessState *symbolic_process_minidump_buffer(const char *buffer,
                                                       uintptr_t length,
                                                       const SymbolicFrameInfoMap *smap);

/*
 * Frees a process state object
 */
void symbolic_process_state_free(SymbolicProcessState *sstate);

/*
 * Converts a dotted path at a line number
 */
SymbolicStr symbolic_proguardmappingview_convert_dotted_path(const SymbolicProguardMappingView *spmv,
                                                             const SymbolicStr *path,
                                                             uint32_t lineno);

/*
 * Frees a proguard mapping view.
 */
void symbolic_proguardmappingview_free(SymbolicProguardMappingView *spmv);

/*
 * Creates a proguard mapping view from bytes.
 *
 * This shares the underlying memory and does not copy it.
 */
SymbolicProguardMappingView *symbolic_proguardmappingview_from_bytes(const char *bytes,
                                                                     uintptr_t len);

/*
 * Creates a proguard mapping view from a path.
 */
SymbolicProguardMappingView *symbolic_proguardmappingview_from_path(const char *path);

/*
 * Returns the UUID
 */
SymbolicUuid symbolic_proguardmappingview_get_uuid(SymbolicProguardMappingView *spmv);

/*
 * Returns true if the mapping file has line infos.
 */
bool symbolic_proguardmappingview_has_line_info(const SymbolicProguardMappingView *spmv);

/*
 * Frees a source map view
 */
void symbolic_sourcemapview_free(const SymbolicSourceMapView *smv);

/*
 * Loads a sourcemap from a JSON byte slice.
 */
SymbolicSourceMapView *symbolic_sourcemapview_from_json_slice(const char *data, uintptr_t len);

/*
 * Return the number of sources.
 */
uint32_t symbolic_sourcemapview_get_source_count(const SymbolicSourceMapView *ssm);

/*
 * Return the source name for an index.
 */
SymbolicStr symbolic_sourcemapview_get_source_name(const SymbolicSourceMapView *ssm,
                                                   uint32_t index);

/*
 * Return the sourceview for a given source.
 */
const SymbolicSourceView *symbolic_sourcemapview_get_sourceview(const SymbolicSourceMapView *ssm,
                                                                uint32_t index);

/*
 * Returns a specific token.
 */
SymbolicTokenMatch *symbolic_sourcemapview_get_token(const SymbolicSourceMapView *ssm,
                                                     uint32_t idx);

/*
 * Returns the number of tokens.
 */
uint32_t symbolic_sourcemapview_get_tokens(const SymbolicSourceMapView *ssm);

/*
 * Looks up a token.
 */
SymbolicTokenMatch *symbolic_sourcemapview_lookup_token(const SymbolicSourceMapView *ssm,
                                                        uint32_t line,
                                                        uint32_t col);

/*
 * Looks up a token.
 */
SymbolicTokenMatch *symbolic_sourcemapview_lookup_token_with_function_name(const SymbolicSourceMapView *ssm,
                                                                           uint32_t line,
                                                                           uint32_t col,
                                                                           const SymbolicStr *minified_name,
                                                                           const SymbolicSourceView *ssv);

/*
 * Returns the underlying source (borrowed).
 */
SymbolicStr symbolic_sourceview_as_str(const SymbolicSourceView *ssv);

/*
 * Frees a source view.
 */
void symbolic_sourceview_free(SymbolicSourceView *ssv);

/*
 * Creates a source view from a given path.
 *
 * This shares the underlying memory and does not copy it if that is
 * possible.  Will ignore utf-8 decoding errors.
 */
SymbolicSourceView *symbolic_sourceview_from_bytes(const char *bytes, uintptr_t len);

/*
 * Returns a specific line.
 */
SymbolicStr symbolic_sourceview_get_line(const SymbolicSourceView *ssv, uint32_t idx);

/*
 * Returns the number of lines.
 */
uint32_t symbolic_sourceview_get_line_count(const SymbolicSourceView *ssv);

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
SymbolicSymCache *symbolic_symcache_from_bytes(const uint8_t *bytes, uintptr_t len);

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
 * Returns the architecture of the symcache.
 */
SymbolicStr symbolic_symcache_get_id(const SymbolicSymCache *scache);

/*
 * Returns the size in bytes of the symcache.
 */
uintptr_t symbolic_symcache_get_size(const SymbolicSymCache *scache);

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
uint32_t symbolic_symcache_latest_file_format_version(void);

/*
 * Looks up a single symbol.
 */
SymbolicLookupResult symbolic_symcache_lookup(const SymbolicSymCache *scache, uint64_t addr);

/*
 * Free a token match
 */
void symbolic_token_match_free(SymbolicTokenMatch *stm);

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
