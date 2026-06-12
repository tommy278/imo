#include <stdio.h>
#include <unistd.h>

// Simulate a running task
int main() {
  while (1) {
    printf("Running task...\n");
    sleep(2);
  }
  return 0;
}
