function archive(a: number, b: number, enabled: boolean, useSaved: boolean, override: boolean) {
  if (!enabled) return 0;

  let current = a - b;
  const saved = current * 3;

  current = a - b;
  if (override) current = 100;

  return useSaved ? saved + current : current;
}
