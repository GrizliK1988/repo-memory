function nullish(a: number, b: number, useSaved: boolean) {
  let selected = a ?? b;
  const saved = selected + 1;

  selected = 100;
  if (useSaved) return saved;
  return selected;
}
