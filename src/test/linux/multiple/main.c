#include "helpers.h"

extern void run_worker_a();

int main() {
  printf("Starting Main\n");
  common_utility(1); // Inlined instance #2 of line 10
  run_worker_a();
  common_utility(2);
  return 0;
}
