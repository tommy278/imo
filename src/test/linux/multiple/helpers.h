#ifndef HELPERS_H
#define HELPERS_H

#include <stdio.h>

// Force the compiler to inline this function everywhere
// Duplicate given line accross multiple locations
__attribute__((always_inline)) inline void common_utility(int worker_id) {
  printf("Utility executing inside worker %d\n", worker_id);
}

#endif
