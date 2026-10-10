function f(again: boolean) {
  let x = 0;
  while (again) {
    x = 1;
    again = false;
  }
  return x;
}
