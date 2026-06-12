#include "helpers.h"

void run_worker_a() {
  printf("Starting Worker A\n");
  common_utility(1); // Inlined instance #1 of line 10
}
