#include "c_string.h"
#include <algorithm>
#include <cstddef>

using std::copy;
using std::string;

char *string_from(const string &str) {
  size_t size = str.size();
  char *result = new char[size + 1];
  if (result == nullptr) {
    return result;
  }

  copy(str.begin(), str.end(), result);
  result[size] = '\0';
  return result;
}

void string_delete(char *str) {
  delete[] str;
}
