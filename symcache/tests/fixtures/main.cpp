#include "main.h"

namespace {
int random() {
  // see https://xkcd.com/221/
  return 4;  // chosen by fair dice roll.
             // guaranteed to be random.
}

int run() {
  // NOTE: Next line covers macro expansion and lambdas
  println("Hello, world!");

  // NOTE: Next line covers function sub-scopes
  while (true) {
    // NOTE: Next line covers template expansion
    int guess = read<int>("Place your guess");
    // NOTE: Next line covers simple return value optimization
    int secret = random();
    if (guess == secret)
      // NOTE: Next line covers nested return value optimization
      return 0;
  }
  // NOTE: Next line might cover dead code elimination
  println("easteregg");
  return 1;
}
}

int main(int argc, char *argv[]) {
  // NOTE: Next line covers aggressive inlining
  return run();
}
