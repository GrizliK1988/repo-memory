function f(again: boolean) {
  let x = 0;
  while (again) {
    x = 2;
    again = false;
  }
  return x;
}
