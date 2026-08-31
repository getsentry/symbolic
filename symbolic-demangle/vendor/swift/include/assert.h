// Our overwrite of assert, this is done so that we get an exception which can be caught before the
// FFI, rather than aborting.
#include <string>
#include <stdexcept>

#undef assert
#define assert(cond)                                                          \
  do {                                                                        \
    if (!(cond))                                                              \
      throw std::logic_error("Assertion failed: (" #cond "), file "           \
                              __FILE__ ", line " + std::to_string(__LINE__)); \
  } while (0)
