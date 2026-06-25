// NOTE: Incompatible with debugger as of right now
// TODO: Make the debugger accept C types without crashing

int main() {
  int x = 10;
  int *p = &x;
  short s = 7;
  float f = 2.8;
  double d = 11.2;
  int arr[5] = {0, 1, 2, 3, 4};
}
