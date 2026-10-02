function f(flag: boolean) {
  let x = 1;
  if (flag) x = 2;
  flag = false;
  if (flag) x = 3;
  return x;
}
