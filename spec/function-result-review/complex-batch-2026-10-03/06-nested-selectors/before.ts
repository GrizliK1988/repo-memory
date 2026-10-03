function selectors(a: number, b: number, enabled: boolean, first: boolean, second: boolean, negate: boolean) {
  if (!enabled) return 0;

  const left = first ? a : b;
  const right = second ? b : a;
  const frozen = left - right;

  return negate ? -frozen : frozen;
}
