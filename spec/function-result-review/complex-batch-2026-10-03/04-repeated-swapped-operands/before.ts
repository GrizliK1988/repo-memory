function repeated(a: number, b: number, reverse: boolean, negate: boolean) {
  let left = a;
  let right = b;
  if (reverse) {
    left = b;
    right = a;
  }

  const delta = left - right;
  const doubled = delta + delta;
  return negate ? -doubled : doubled;
}
