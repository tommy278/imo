#include <stdio.h>
#include <unistd.h>

int main() {
  while (1) {
    printf("Running task...\n");
    sleep(2);
    for (int i = 0; i < 5; i++) {
      printf("%i\n", i);
    }
  }
  return 0;
}
