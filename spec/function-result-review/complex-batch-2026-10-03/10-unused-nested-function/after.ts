function outer(a: number, b: number, flag: boolean) {
  function helper(value: number) {
    let candidate = value - a;
    if (flag) candidate = candidate + b;
    return candidate * 2;
  }

  let current = a - b;
  const saved = current;
  current = b - a;
  if (flag) return saved;
  return current;
}
