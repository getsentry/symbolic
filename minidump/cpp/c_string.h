#ifndef SENTRY_C_STRING_H
#define SENTRY_C_STRING_H

#ifdef __cplusplus
#include <string>

extern "C" {

/// Creates an owned copy of the string's conents as char pointer.
/// This is useful when returning strings from extern "C" functions.
char *string_from(const std::string &str);
#endif

/// Releases memory of the string. Assumes ownership of the pointer.
void string_delete(char *str);

#ifdef __cplusplus
}
#endif

#endif
