function f(again: boolean, boost: boolean) {
  let x = 0;
  while (again) {
    x = 1;
    if (boost) {
      x = 2;
    }
    again = false;
  }
  return x;
}
