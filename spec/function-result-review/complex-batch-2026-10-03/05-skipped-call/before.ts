function skipped(a: number, b: number, enabled: boolean, chooseCaptured: boolean) {
  const captured = enabled && mystery(a);
  if (!enabled) return a + b;

  const fallback = b - a;
  return chooseCaptured ? captured : fallback;
}
