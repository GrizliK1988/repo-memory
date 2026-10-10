function f(count: number, step: number) {
  let remaining = count;
  let x = 0;
  while (remaining > 0) {
    x = x + step + 1;
    remaining = remaining - 1;
  }
  return x;
}
