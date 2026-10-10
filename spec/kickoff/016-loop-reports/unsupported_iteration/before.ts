function f(again: boolean, items: number[]) {
  let x = 0;
  while (again) {
    x = 1;
    again = false;
  }
  return x;
}
