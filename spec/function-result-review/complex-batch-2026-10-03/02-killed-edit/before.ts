function killed(a: number, b: number, first: boolean, second: boolean) {
  let value = a + b;
  const unused = value * 2;

  if (first) value = a;
  else value = b;

  if (second) value = 10;
  else value = 20;

  const copy = value;
  return copy;
}
