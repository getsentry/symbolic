#include <stdio.h>

#define println(s) ([] { printf("%s\n", s); })()

template <typename T>
T read(const char *query) {
  printf("%s: ", query);
  int val;
  scanf("%d", &val);
  return (T)val;
}
