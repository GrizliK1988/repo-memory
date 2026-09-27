# Function result report review examples

Date: 2026-09-27. These examples exercise the currently supported synchronous subset; `unknown` is a boundary case. Product source and existing tests are unchanged.

The expectations below were derived from source semantics before running the analyzer. Inputs are intentionally unconstrained: conditions use JavaScript truthiness and are not assumed to be Boolean.

## literal: Literal replacement

Expected: 1 → 2 (different) for all inputs.

### before

```typescript
function result() {
  return 1;
}
```

### after

```typescript
function result() {
  return 2;
}
```

## copy: Changed source of a saved copy

Expected: 1 → 2 (different). The later write `x = 9` does not change `saved`.

### before

```typescript
function result() {
  let x = 1;
  const saved = x;
  x = 9;
  return saved;
}
```

### after

```typescript
function result() {
  let x = 2;
  const saved = x;
  x = 9;
  return saved;
}
```

## priority: Reordered conditional writes

Expected: 2 → 1 only when `a && b`; equal otherwise.

### before

```typescript
function result(a, b) {
  let x = 0;
  if (a) x = 1;
  if (b) x = 2;
  return x;
}
```

### after

```typescript
function result(a, b) {
  let x = 0;
  if (b) x = 2;
  if (a) x = 1;
  return x;
}
```

## guard: Added condition after early returns

Expected: `"ok"` → `"pending"` only when `enabled && !blocked && ready && !approved`; equal for all other inputs.

### before

```typescript
function result(enabled, blocked, ready, approved) {
  if (!enabled) return "disabled";
  if (blocked) return "blocked";
  if (ready) return "ok";
  return "pending";
}
```

### after

```typescript
function result(enabled, blocked, ready, approved) {
  if (!enabled) return "disabled";
  if (blocked) return "blocked";
  if (ready && approved) return "ok";
  return "pending";
}
```

## guard_version: Overwritten guard value

Expected: `flag`: `"yes"` → `"no"`; `!flag`: `"no"` → `"yes"`.

### before

```typescript
function result(flag) {
  let g = flag;
  if (g) return "yes";
  return "no";
}
```

### after

```typescript
function result(flag) {
  let g = flag;
  g = !g;
  if (g) return "yes";
  return "no";
}
```

## early_return: Added early return

Expected: `stop`: `"normal"` → `"early"`; `!stop`: equal.

### before

```typescript
function result(stop) {
  return "normal";
}
```

### after

```typescript
function result(stop) {
  if (stop) return "early";
  return "normal";
}
```

## fallthrough: Replaced implicit undefined

Expected: `!flag`: `undefined` → `2`; `flag`: equal.

### before

```typescript
function result(flag) {
  if (flag) return 1;
}
```

### after

```typescript
function result(flag) {
  if (flag) return 1;
  return 2;
}
```

## short_circuit: Changed right operand of `&&`

Expected: for truthy `flag`, `"old"` → `"new"`; for falsy `flag`, the same original `flag` value is returned and the result is equal (it need not be `false`).

### before

```typescript
function result(flag) {
  return flag && "old";
}
```

### after

```typescript
function result(flag) {
  return flag && "new";
}
```

## expression: Changed computation

Expected: `changed`, `input + 1` → `input + 2`. Do not claim `different` for every JavaScript input.

### before

```typescript
function result(input) {
  return input + 1;
}
```

### after

```typescript
function result(input) {
  return input + 2;
}
```

## overwritten: Control: changed write is overwritten

Expected: 9 → 9 (equal).

### before

```typescript
function result() {
  let x = 1;
  x = 9;
  return x;
}
```

### after

```typescript
function result() {
  let x = 2;
  x = 9;
  return x;
}
```

## equal_control: Control: changed guard with the same result

Expected: 1 → 1 (equal) for all inputs; report the control change separately.

### before

```typescript
function result(flag) {
  if (flag) return 1;
  return 1;
}
```

### after

```typescript
function result(flag) {
  if (!flag) return 1;
  return 1;
}
```

## unknown: Control: unsupported call on one branch

Expected: `flag`: 1 → 2 (different); `!flag`: unknown completion. Coverage is partial, not unchanged.

### before

```typescript
function result(flag) {
  if (flag) return 1;
  mystery();
  return 3;
}
```

### after

```typescript
function result(flag) {
  if (flag) return 2;
  mystery();
  return 3;
}
```

## known_expression: Changed computation with a known numeric source

Expected: `changed`, computation `input + 1` → `input + 2`; the current contract does not require evaluating arithmetic on literals. The concrete semantic result is 5 → 6.

### before

```typescript
function result() {
  let input = 4;
  return input + 1;
}
```

### after

```typescript
function result() {
  let input = 4;
  return input + 2;
}
```
