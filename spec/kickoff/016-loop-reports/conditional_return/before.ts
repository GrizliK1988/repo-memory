function f(again: boolean, boost: boolean) {
  let x = 0;
  while (again) {
    x = 1;
    again = false;
  }
  return x;
}
