function accumulate(count: number, step: number) {
  let remaining = count;
  let total = 0;
  const increment = step + 1;

  while (remaining > 0) {
    total = total + increment;
    remaining = remaining - 1;
  }

  return total;
}
