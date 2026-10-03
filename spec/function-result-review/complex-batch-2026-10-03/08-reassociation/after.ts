function association(a: number, b: number, c: number, skip: boolean, negate: boolean) {
  const total = a + (b + c);
  const doubled = total * 2;

  if (skip) return 0;
  return negate ? -doubled : doubled;
}
