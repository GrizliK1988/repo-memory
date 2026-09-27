# Проверка отчётов о результатах функций

Дата: 2026-09-27. Примеры для уже поддерживаемого синхронного подмножества; `unknown` — контроль границы. Исходники продукта и существующие тесты не меняются.

Ожидания ниже заданы по семантике исходников до запуска анализатора. Параметры намеренно без ограничений: условия означают JS truthiness, а не обязательный Boolean.

## literal: Замена литерала

Ожидание: Для любых входов 1 → 2: different.

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

## copy: Изменение источника сохранённой копии

Ожидание: 1 → 2: different. Последующая запись x = 9 не меняет saved.

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

## priority: Перестановка условных записей

Ожидание: Только при a && b: 2 → 1. Иначе equal.

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

## guard: Дополнительное условие после ранних возвратов

Ожидание: Только enabled && !blocked && ready && !approved: "ok" → "pending". Остальные входы equal.

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

## guard_version: Перезапись значения условия

Ожидание: flag: "yes" → "no"; !flag: "no" → "yes".

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

## early_return: Добавление раннего возврата

Ожидание: stop: "normal" → "early"; !stop: equal.

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

## fallthrough: Замена неявного undefined

Ожидание: !flag: undefined → 2; flag: equal.

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

## short_circuit: Замена результата правой ветки &&

Ожидание: Truthy flag: "old" → "new"; falsy flag: тот же исходный flag, equal (не обязательно false).

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

## expression: Изменение вычисления

Ожидание: changed: input + 1 → input + 2. Не утверждать different для любых JS-входов.

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

## overwritten: Контроль: изменённая запись убита

Ожидание: 9 → 9: equal.

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

## equal_control: Контроль: новое условие с тем же результатом

Ожидание: 1 → 1: equal на всех входах, изменение управления отдельно.

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

## unknown: Контроль: неподдержанный вызов в одной ветке

Ожидание: flag: 1 → 2 different; !flag: unknown completion; покрытие partial, не unchanged.

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

## known_expression: Изменение вычисления с известным числовым источником

Ожидание: `changed`, вычисление `input + 1 → input + 2`; текущий контракт не требует вычислять арифметику литералов. Семантически 5 → 6.

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
