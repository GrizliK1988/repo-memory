function versioned(a: number, b: number, first: boolean, second: boolean) {
  let decision = first;
  const original = decision;
  decision = second;

  let value = a;
  if (original) value = a + b;
  if (decision) value = b - a;

  return value;
}
