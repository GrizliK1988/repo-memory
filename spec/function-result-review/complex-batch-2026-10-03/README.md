# Сравнение возвратного data flow: 10 сложных примеров

Текущее состояние после исправления примера 3 и повторного запуска всех 10 команд:
[дополнение об атрибуции и оставшихся задачах](FOLLOW_UP.md). Ниже сохранён исходный
отчёт предыдущего прогона.

Дата отчёта: 2026-10-03. Проверено текущее рабочее дерево с предыдущими изменениями анализатора и текстового отчёта. В рамках этого прогона код анализатора не исправлялся.

В каждом разделе приведены полный исходный код до и после, фактически запущенная команда, её stdout без изменений, независимое ожидаемое поведение и возможные проблемы. `report.txt` хранит оригинальный stdout, `stderr.log` — полный stderr Cargo, `run.json` — аргументы, рабочий каталог и exit code, `full.json` — исходный evidence. Строка `Evidence: full.json` относится к каталогу соответствующего примера.

По умолчанию CLI получает unrestricted positional inputs: аннотации TypeScript не превращаются в runtime-ограничения. `Changed` означает изменение разрешённого вычисления/источников; он не требует доказательства неравенства значений. `Unknown` из-за runtime-типа может сосуществовать с установленной неизменностью data flow. Сравниваются нормальные возвраты, а не полная эквивалентность всех эффектов программы.

Все **10 команд завершились с exit code 0**. Дополнительно выполнено **45 конкретных сценариев JavaScript**; их результаты совпали с независимо заданными ожидаемыми значениями. Это конечные контрольные примеры, не доказательство для всех возможных входов. Для примера 5 контрольный runtime отдельно задаёт `mystery(x)=x*3`; CLI эту модель не получал. Для примера 7 `null` намеренно передан вопреки аннотации `number`: это допустимый unrestricted-вход анализа. `-0` сохранён отдельно от `0`.

## Наблюдения, требующие внимания

1. **Известная единственная неизменная ветка пропадает из короткого текста** — примеры 6, 7 и 8. В полном JSON соответствующие `Equal`-регионы есть. Это подтверждённая проблема отображения.
2. **Атрибуция и группировка одного редактирования остаются недостаточно точными** — примеры 3, 4, 6 и 8. Особенно заметны четыре или восемь повторов `Edit attribution uncertain`. Это возможность улучшения; неопределённую атрибуцию нельзя просто объявить доказанной.
3. **Причина Unknown теряет конкретное препятствие** — неизвестный вызов в примере 5 и неподдерживаемый цикл в примере 9. В полном JSON эта информация сохранена.
4. **`??` теряет точность на falsy-входах** — пример 7. Нельзя подменять nullish проверкой truthiness; отдельно стоит изучить одинаковый независимый возврат 100 после перезаписи.
5. **Одна строка Coverage смешивает впечатление о полноте data flow с runtime-доказательствами** — например, 1 и 10. Partial там не опровергает установленную область изменения/неизменности data flow.

## Сводка

| № | Проверка | Результат проверки | Что стоит улучшить |
|---|---|---|---|
| [1](#1-01-captured-ternary) | Сохранённая копия и перезаписи | Область изменения правильная | Пояснение Coverage |
| [2](#2-02-killed-edit) | Уничтоженная запись | Изменение правильно исключено | Раскрытие правила выбора 10/20 |
| [3](#3-03-guard-version) | Версия guard | Области правильные | Причина control и атрибуция |
| [4](#4-04-repeated-swapped-operands) | Повторное чтение и перестановка | Все четыре изменения найдены | 4 повтора и атрибуция |
| [5](#5-05-skipped-call) | Пропущенный/выполненный вызов | Консервативный Unknown обоснован | Указать неизвестный вызов |
| [6](#6-06-nested-selectors) | Вложенные селекторы | 8 изменений найдены | Пропущен !enabled; 8 повторов |
| [7](#7-07-nullish-and-overwrite) | Nullish и перезапись | Изменение видно на truthy a | Пропуск Equal; точность на falsy a |
| [8](#8-08-reassociation) | Перегруппировка арифметики | Изменения правильно найдены | Пропущен skip; атрибуция |
| [9](#9-09-loop-boundary) | Цикл | Ожидаемый Unknown | Указать неподдерживаемый while |
| [10](#10-10-unused-nested-function) | Невызванная вложенная функция | Изменение правильно исключено | Объяснить исключение diff |

<a id="1-01-captured-ternary"></a>

## 1. Сохранённое вычисление, перезаписи и ранний выход

Артефакты: [before.ts](01-captured-ternary/before.ts), [after.ts](01-captured-ternary/after.ts), [report.txt](01-captured-ternary/report.txt), [full.json](01-captured-ternary/full.json), [stderr.log](01-captured-ternary/stderr.log), [run.json](01-captured-ternary/run.json).

### До

```typescript
function archive(a: number, b: number, enabled: boolean, useSaved: boolean, override: boolean) {
  if (!enabled) return 0;

  let current = a + b;
  const saved = current * 3;

  current = a - b;
  if (override) current = 100;

  return useSaved ? saved + current : current;
}
```

### После

```typescript
function archive(a: number, b: number, enabled: boolean, useSaved: boolean, override: boolean) {
  if (!enabled) return 0;

  let current = a - b;
  const saved = current * 3;

  current = a - b;
  if (override) current = 100;

  return useSaved ? saved + current : current;
}
```

### Ожидаемое поведение

Data flow меняется только при enabled && useSaved. Сохранённая копия переживает перезаписи current; override влияет на окружающее вычисление, но не на область изменения.

### Команда

Рабочий каталог: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/01-captured-ternary`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function archive --format text --evidence-out full.json
```

### Вывод команды — stdout без изменений

```text
Function result: archive
Edit: (a + b) -> (a - b) (before snippet.ts:4; after snippet.ts:4)
Changed computation for saved:
  ((a + b) * 3) -> ((a - b) * 3) (Changed)
  Reaches return useSaved ? saved + current : current
Before shared writes: snippet.ts:4, snippet.ts:5
After shared writes: snippet.ts:4, snippet.ts:5
Return under enabled && useSaved && !override: (saved + (a - b))
  Context writes: before snippet.ts:7; after snippet.ts:7
Return under enabled && useSaved && override: (saved + 100)
  Context writes: before snippet.ts:8; after snippet.ts:8
Unchanged data flow under !enabled || !useSaved (runtime value equality unresolved in some regions)
Coverage: result comparison Partial; source alignment Partial
Evidence: full.json (ifds-function-cf15c3064f8f3fba)
```

### Проверка и возможные проблемы

Ожидаемая область изменения и сохранение старой версии current показаны правильно. Общая изменённая цепочка saved выведена один раз; условия override сохранены отдельно.

- Явной ошибки data flow в проверенных регионах не найдено.
- Partial относится к сравнению runtime-значений в неизменном вычислении a - b. Оно не означает, что область изменённого data flow осталась неизвестной. Для пользователя это всё ещё не очень очевидно из общей строки Coverage.

### Конкретные контрольные сценарии

| Аргументы по порядку параметров | До | После |
|---|---|---|
| `[5, 2, false, true, true]` | `0` | `0` |
| `[5, 2, true, false, false]` | `3` | `3` |
| `[5, 2, true, false, true]` | `100` | `100` |
| `[5, 2, true, true, false]` | `24` | `12` |
| `[5, 2, true, true, true]` | `121` | `109` |

<a id="2-02-killed-edit"></a>

## 2. Изменённая запись уничтожена двумя уровнями присваиваний

Артефакты: [before.ts](02-killed-edit/before.ts), [after.ts](02-killed-edit/after.ts), [report.txt](02-killed-edit/report.txt), [full.json](02-killed-edit/full.json), [stderr.log](02-killed-edit/stderr.log), [run.json](02-killed-edit/run.json).

### До

```typescript
function killed(a: number, b: number, first: boolean, second: boolean) {
  let value = a + b;
  const unused = value * 2;

  if (first) value = a;
  else value = b;

  if (second) value = 10;
  else value = 20;

  const copy = value;
  return copy;
}
```

### После

```typescript
function killed(a: number, b: number, first: boolean, second: boolean) {
  let value = a - b;
  const unused = value * 2;

  if (first) value = a;
  else value = b;

  if (second) value = 10;
  else value = 20;

  const copy = value;
  return copy;
}
```

### Ожидаемое поведение

Изменение не достигает возврата ни в одной ветке: second выбирает 10 или 20. Сравниваются нормальные возвраты, а не все возможные эффекты выполнения арифметики.

### Команда

Рабочий каталог: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/02-killed-edit`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function killed --format text --evidence-out full.json
```

### Вывод команды — stdout без изменений

```text
Function result: killed
Normal results unchanged on common inputs: always (under the recorded entry assumptions).
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-cdd967feba9cf6b7)
```

### Проверка и возможные проблемы

Изменённое вычисление и unused не достигают возврата. Команда правильно сообщает неизменность нормальных результатов и Complete для сравнения.

- Явной ошибки в проверенных регионах не найдено. Это полезный отрицательный контроль: отличие исходного текста само по себе не является изменением возвращаемого data flow.
- Короткий отчёт не раскрывает правило second ? 10 : 20. Это возможное улучшение полноты объяснения, а не неправильное сравнение.

### Конкретные контрольные сценарии

| Аргументы по порядку параметров | До | После |
|---|---|---|
| `[5, 2, false, false]` | `20` | `20` |
| `[5, 2, false, true]` | `10` | `10` |
| `[5, 2, true, false]` | `20` | `20` |
| `[5, 2, true, true]` | `10` | `10` |

<a id="3-03-guard-version"></a>

## 3. Снимок условия и новая версия изменяемого guard

Артефакты: [before.ts](03-guard-version/before.ts), [after.ts](03-guard-version/after.ts), [report.txt](03-guard-version/report.txt), [full.json](03-guard-version/full.json), [stderr.log](03-guard-version/stderr.log), [run.json](03-guard-version/run.json).

### До

```typescript
function versioned(a: number, b: number, first: boolean, second: boolean) {
  let decision = first;
  const original = decision;
  decision = !decision;

  let value = a;
  if (original) value = a + b;
  if (decision) value = b - a;

  return value;
}
```

### После

```typescript
function versioned(a: number, b: number, first: boolean, second: boolean) {
  let decision = first;
  const original = decision;
  decision = second;

  let value = a;
  if (original) value = a + b;
  if (decision) value = b - a;

  return value;
}
```

### Ожидаемое поведение

До изменения decision равен !first, после — second. Data flow меняется при (!first && !second) || (first && second); original остаётся снимком first.

### Команда

Рабочий каталог: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/03-guard-version`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function versioned --format text --evidence-out full.json
```

### Вывод команды — stdout без изменений

```text
Function result: versioned
Unchanged data flow under (!first && second) || (first && !second) (runtime value equality unresolved)
Edit attribution uncertain
Control changed (guard correspondence unresolved)
Changed computation under first && second:
  (a + b) -> (b - a) (Changed)
  Reaches return value
Before contributing writes: snippet.ts:7
After contributing writes: snippet.ts:8
Edit attribution uncertain
Changed computation under !first && !second:
  (b - a) -> a (Changed)
  Reaches return value
Before contributing writes: snippet.ts:8
After contributing writes: snippet.ts:6
Coverage: result comparison Partial; source alignment Partial
Evidence: full.json (ifds-function-b3094789d161c137)
```

### Проверка и возможные проблемы

Изменённые регионы first && second и !first && !second определены правильно. Неизменные регионы собраны в точную общую область (!first && second) || (first && !second). Сохранённый original не смешан с новой версией decision.

- Недостаточно конкретное объяснение изменения управления: Control changed (guard correspondence unresolved) не показывает установленную зависимость decision: !first -> second. Оба источника можно проследить в full.json, но текущая текстовая политика умеет конкретно объяснять более узкий класс изменений guard.
- Edit attribution uncertain повторяется дважды. Для человека причина одна — запись decision на строке 4; для анализатора сопоставление редактирования источника пока не доказано. Это возможное улучшение атрибуции и группировки, а не основание объявлять её доказанной без проверки.

### Конкретные контрольные сценарии

| Аргументы по порядку параметров | До | После |
|---|---|---|
| `[5, 2, false, false]` | `-3` | `5` |
| `[5, 2, false, true]` | `-3` | `-3` |
| `[5, 2, true, false]` | `7` | `7` |
| `[5, 2, true, true]` | `7` | `-3` |

<a id="4-04-repeated-swapped-operands"></a>

## 4. Перестановка операндов через копии и повторное чтение

Артефакты: [before.ts](04-repeated-swapped-operands/before.ts), [after.ts](04-repeated-swapped-operands/after.ts), [report.txt](04-repeated-swapped-operands/report.txt), [full.json](04-repeated-swapped-operands/full.json), [stderr.log](04-repeated-swapped-operands/stderr.log), [run.json](04-repeated-swapped-operands/run.json).

### До

```typescript
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
```

### После

```typescript
function repeated(a: number, b: number, reverse: boolean, negate: boolean) {
  let left = a;
  let right = b;
  if (reverse) {
    left = b;
    right = a;
  }

  const delta = right - left;
  const doubled = delta + delta;
  return negate ? -doubled : doubled;
}
```

### Ожидаемое поведение

Data flow меняется во всех четырёх сочетаниях reverse/negate: важны роли операндов, а не набор источников {a,b}. Повторное чтение delta не должно смешивать разные версии записи. Изменение не доказывает неравенство значений при любых входах.

### Команда

Рабочий каталог: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/04-repeated-swapped-operands`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function repeated --format text --evidence-out full.json
```

### Вывод команды — stdout без изменений

```text
Function result: repeated
Edit attribution uncertain
Changed computation under !reverse && !negate:
  ((a - b) + (a - b)) -> ((b - a) + (b - a)) (Changed)
  Reaches return negate ? -doubled : doubled
Before contributing writes: snippet.ts:2, snippet.ts:3, snippet.ts:9, snippet.ts:10
After contributing writes: snippet.ts:2, snippet.ts:3, snippet.ts:9, snippet.ts:10
Edit attribution uncertain
Changed computation under reverse && !negate:
  ((b - a) + (b - a)) -> ((a - b) + (a - b)) (Changed)
  Reaches return negate ? -doubled : doubled
Before contributing writes: snippet.ts:5, snippet.ts:6, snippet.ts:9, snippet.ts:10
After contributing writes: snippet.ts:5, snippet.ts:6, snippet.ts:9, snippet.ts:10
Edit attribution uncertain
Changed computation under !reverse && negate:
  (-((a - b) + (a - b))) -> (-((b - a) + (b - a))) (Changed)
  Reaches return negate ? -doubled : doubled
Before contributing writes: snippet.ts:2, snippet.ts:3, snippet.ts:9, snippet.ts:10
After contributing writes: snippet.ts:2, snippet.ts:3, snippet.ts:9, snippet.ts:10
Edit attribution uncertain
Changed computation under reverse && negate:
  (-((b - a) + (b - a))) -> (-((a - b) + (a - b))) (Changed)
  Reaches return negate ? -doubled : doubled
Before contributing writes: snippet.ts:5, snippet.ts:6, snippet.ts:9, snippet.ts:10
After contributing writes: snippet.ts:5, snippet.ts:6, snippet.ts:9, snippet.ts:10
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-3f1a4f08e3513034)
```

### Проверка и возможные проблемы

Все четыре региона правильно помечены Changed. В деревьях сохранены порядок операндов, повторное использование delta и смена знака. Контроль a=b=2 даёт 0 -> 0: Changed здесь означает изменение вычисления, а не обязательное неравенство значений.

- Повторное раскрытие delta создаёт четыре длинных блока и четыре Edit attribution uncertain, хотя пользователь изменил одну запись на строке 9.
- Атрибуция перестановки операндов остаётся неясной: текущая проверка уникального изменения не объединяет несколько изменённых чтений внутри одного переставленного выражения. Полезно распознавать такую перестановку с сохранением версий и ролей операндов.
- Можно добавить общую область Changed: always и одно объяснение delta, оставив конкретные вычисления каждой ветки в подробном режиме. Это предложение по отображению, не уже установленная общая формула возвращаемого значения.

### Конкретные контрольные сценарии

| Аргументы по порядку параметров | До | После |
|---|---|---|
| `[5, 2, false, false]` | `6` | `-6` |
| `[5, 2, true, false]` | `-6` | `6` |
| `[5, 2, false, true]` | `-6` | `6` |
| `[5, 2, true, true]` | `6` | `-6` |
| `[2, 2, false, false]` | `0` | `0` |

<a id="5-05-skipped-call"></a>

## 5. Короткое замыкание и выполненный неизвестный вызов

Артефакты: [before.ts](05-skipped-call/before.ts), [after.ts](05-skipped-call/after.ts), [report.txt](05-skipped-call/report.txt), [full.json](05-skipped-call/full.json), [stderr.log](05-skipped-call/stderr.log), [run.json](05-skipped-call/run.json).

### До

```typescript
function skipped(a: number, b: number, enabled: boolean, chooseCaptured: boolean) {
  const captured = enabled && mystery(a);
  if (!enabled) return a + b;

  const fallback = b - a;
  return chooseCaptured ? captured : fallback;
}
```

### После

```typescript
function skipped(a: number, b: number, enabled: boolean, chooseCaptured: boolean) {
  const captured = enabled && mystery(b);
  if (!enabled) return a + b;

  const fallback = b - a;
  return chooseCaptured ? captured : fallback;
}
```

### Ожидаемое поведение

При !enabled вызов пропущен и data flow возврата не меняется. При enabled результат/завершение неизвестны даже при !chooseCaptured: вызов уже был выполнен. Для числовых контрольных сценариев отдельно задана модель mystery(x)=x*3; CLI этой модели не получает.

### Команда

Рабочий каталог: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/05-skipped-call`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function skipped --format text --evidence-out full.json
```

### Вывод команды — stdout без изменений

```text
Function result: skipped
Unchanged data flow under !enabled: (a + b) (runtime value equality unresolved)
Edit attribution uncertain
Unknown under enabled: normal completion or result selection is unresolved
Coverage: result comparison Partial; source alignment Partial; before analysis Partial; after analysis Partial
Evidence: full.json (ifds-function-2ed9604500d4d8b6)
```

### Проверка и возможные проблемы

При !enabled вызов действительно пропускается; неизменный data flow a + b показан. При enabled команда сохраняет Unknown и не приписывает вызову доказанное изменение возвращённого значения. Даже при !chooseCaptured неизвестный вызов уже выполнялся.

- Причина Unknown слишком общая: normal completion or result selection is unresolved. В полном evidence есть unsupported call_expression на строке 2 и по два наблюдения с unknown_completion_before_return=true на каждой стороне. Пользовательский текст теряет указание на неизвестный вызов.
- Это корректная консервативность результата при недостаточном объяснении причины. Числовая модель mystery(x)=x*3 в контрольных сценариях не передавалась CLI и не даёт права заменить Unknown на Changed/Equal.

### Конкретные контрольные сценарии

| Аргументы по порядку параметров | До | После |
|---|---|---|
| `[5, 2, false, false]` | `7` | `7` |
| `[5, 2, false, true]` | `7` | `7` |
| `[5, 2, true, false]` | `-3` | `-3` |
| `[5, 2, true, true]` | `15` | `6` |

<a id="6-06-nested-selectors"></a>

## 6. Два условных источника, ранний выход и смена знака

Артефакты: [before.ts](06-nested-selectors/before.ts), [after.ts](06-nested-selectors/after.ts), [report.txt](06-nested-selectors/report.txt), [full.json](06-nested-selectors/full.json), [stderr.log](06-nested-selectors/stderr.log), [run.json](06-nested-selectors/run.json).

### До

```typescript
function selectors(a: number, b: number, enabled: boolean, first: boolean, second: boolean, negate: boolean) {
  if (!enabled) return 0;

  const left = first ? a : b;
  const right = second ? b : a;
  const frozen = left - right;

  return negate ? -frozen : frozen;
}
```

### После

```typescript
function selectors(a: number, b: number, enabled: boolean, first: boolean, second: boolean, negate: boolean) {
  if (!enabled) return 0;

  const left = first ? a : b;
  const right = second ? a : b;
  const frozen = left - right;

  return negate ? -frozen : frozen;
}
```

### Ожидаемое поведение

При enabled выбранный правый операнд меняется во всех сочетаниях first/second/negate. При !enabled возврат 0 не меняется. Источники после изменения могут совпасть внутри одного вычисления — это не повод потерять роли операндов.

### Команда

Рабочий каталог: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/06-nested-selectors`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function selectors --format text --evidence-out full.json
```

### Вывод команды — stdout без изменений

```text
Function result: selectors
Edit attribution uncertain
Changed computation under enabled && first && !second && !negate:
  (a - a) -> (a - b) (Changed)
  Reaches return negate ? -frozen : frozen
Before contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
After contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
Edit attribution uncertain
Changed computation under enabled && first && second && !negate:
  (a - b) -> (a - a) (Changed)
  Reaches return negate ? -frozen : frozen
Before contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
After contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
Edit attribution uncertain
Changed computation under enabled && !first && !second && !negate:
  (b - a) -> (b - b) (Changed)
  Reaches return negate ? -frozen : frozen
Before contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
After contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
Edit attribution uncertain
Changed computation under enabled && !first && second && !negate:
  (b - b) -> (b - a) (Changed)
  Reaches return negate ? -frozen : frozen
Before contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
After contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
Edit attribution uncertain
Changed computation under enabled && first && !second && negate:
  (-(a - a)) -> (-(a - b)) (Changed)
  Reaches return negate ? -frozen : frozen
Before contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
After contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
Edit attribution uncertain
Changed computation under enabled && first && second && negate:
  (-(a - b)) -> (-(a - a)) (Changed)
  Reaches return negate ? -frozen : frozen
Before contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
After contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
Edit attribution uncertain
Changed computation under enabled && !first && !second && negate:
  (-(b - a)) -> (-(b - b)) (Changed)
  Reaches return negate ? -frozen : frozen
Before contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
After contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
Edit attribution uncertain
Changed computation under enabled && !first && second && negate:
  (-(b - b)) -> (-(b - a)) (Changed)
  Reaches return negate ? -frozen : frozen
Before contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
After contributing writes: snippet.ts:4, snippet.ts:5, snippet.ts:6
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-9989c5f4a604d3d5)
```

### Проверка и возможные проблемы

Восемь изменённых регионов enabled с разными first/second/negate раскрыты правильно. Полный JSON также устанавливает Equal для !enabled с возвратом 0.

- Подтверждённый пропуск в коротком тексте: ветка !enabled вообще не показана. В comparison.regions[0] есть Equal, value_dependency_changed=false и условие {2:false}; это известная неизменная ветка, а не Unknown.
- Порог группировки неизменных регионов требует минимум два региона. Единственный Equal без control_changed затем не попадает ни в summary, ни в обычный цикл эффектов. Поэтому успешное сравнение само по себе не гарантирует полноту короткого отчёта.
- Одна замена источников в const right на строке 5 порождает восемь сообщений Edit attribution uncertain и восемь одинаковых списков строк записей. Нужны более точная атрибуция условного выбора и общая область изменения enabled.
- Не следует автоматически упрощать a - a до 0 на unrestricted-входах. Даже на обычных числовых примерах -(a - a) даёт -0; отрицательный ноль отдельно сохранён в runtime-checks.json.

### Конкретные контрольные сценарии

| Аргументы по порядку параметров | До | После |
|---|---|---|
| `[5, 2, false, true, true, false]` | `0` | `0` |
| `[5, 2, true, true, false, false]` | `0` | `3` |
| `[5, 2, true, true, true, false]` | `3` | `0` |
| `[5, 2, true, false, false, false]` | `-3` | `0` |
| `[5, 2, true, false, true, false]` | `0` | `-3` |
| `[5, 2, true, true, true, true]` | `-3` | `-0` |

<a id="7-07-nullish-and-overwrite"></a>

## 7. Nullish-выбор, сохранённая копия и независимый возврат

Артефакты: [before.ts](07-nullish-and-overwrite/before.ts), [after.ts](07-nullish-and-overwrite/after.ts), [report.txt](07-nullish-and-overwrite/report.txt), [full.json](07-nullish-and-overwrite/full.json), [stderr.log](07-nullish-and-overwrite/stderr.log), [run.json](07-nullish-and-overwrite/run.json).

### До

```typescript
function nullish(a: number, b: number, useSaved: boolean) {
  let selected = a ?? b;
  const saved = selected + 1;

  selected = 100;
  if (useSaved) return saved;
  return selected;
}
```

### После

```typescript
function nullish(a: number, b: number, useSaved: boolean) {
  let selected = a ?? b;
  const saved = selected + 2;

  selected = 100;
  if (useSaved) return saved;
  return selected;
}
```

### Ожидаемое поведение

Логически вычисление saved меняется при useSaved, а при !useSaved возвращается 100. На unrestricted-входе falsy не означает nullish: 0 должен остаться выбранным a, null должен выбрать b. Консервативный Unknown при неясном выборе — ограничение точности, а не доказательство отсутствия изменения.

### Команда

Рабочий каталог: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/07-nullish-and-overwrite`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function nullish --format text --evidence-out full.json
```

### Вывод команды — stdout без изменений

```text
Function result: nullish
Edit: 1 -> 2 (before snippet.ts:3; after snippet.ts:3)
Changed computation under a && useSaved:
  (a + 1) -> (a + 2) (Changed)
  Reaches return saved
Before contributing writes: snippet.ts:2, snippet.ts:3
After contributing writes: snippet.ts:2, snippet.ts:3
Edit attribution uncertain
Unknown under !a: normal completion or result selection is unresolved
Coverage: result comparison Partial; source alignment Partial
Evidence: full.json (ifds-function-29c914b997a7bb2c)
```

### Проверка и возможные проблемы

На truthy a команда правильно показывает изменение saved при useSaved. На falsy a сохраняется Unknown: одной truthiness недостаточно, чтобы отличить 0 от null. Контрольные сценарии подтверждают, что 0 выбирает a, а null выбирает b.

- Подтверждённый пропуск в коротком тексте: установленный Equal для a && !useSaved отсутствует. Он есть в comparison.regions[1], и обе стороны возвращают 100.
- Потеря точности при !a: Unknown охватывает и !useSaved, хотя выбранная копия уже перезаписана числом 100. Для этого случая можно отдельно исследовать объединение одинаковых нормальных результатов из нескольких вариантов nullish-выбора; нельзя просто считать falsy равным nullish.
- Изменение data flow при a=0 и useSaved видно при непосредственном выполнении (1 -> 2), но CLI не устанавливает соответствующую ветку. Это консервативная неполнота модели входных условий, а не ложное утверждение о равенстве.
- Запись условия !a в отчёте означает falsy a, а не a == null. Пояснение Unknown не раскрывает эту границу, поэтому возможна неверная интерпретация пользователем.

### Конкретные контрольные сценарии

| Аргументы по порядку параметров | До | После |
|---|---|---|
| `[5, 2, true]` | `6` | `7` |
| `[0, 2, true]` | `1` | `2` |
| `[null, 2, true]` | `3` | `4` |
| `[null, 2, false]` | `100` | `100` |
| `[0, 2, false]` | `100` | `100` |

<a id="8-08-reassociation"></a>

## 8. Перегруппировка арифметики с округлением и ранним возвратом

Артефакты: [before.ts](08-reassociation/before.ts), [after.ts](08-reassociation/after.ts), [report.txt](08-reassociation/report.txt), [full.json](08-reassociation/full.json), [stderr.log](08-reassociation/stderr.log), [run.json](08-reassociation/run.json).

### До

```typescript
function association(a: number, b: number, c: number, skip: boolean, negate: boolean) {
  const total = (a + b) + c;
  const doubled = total * 2;

  if (skip) return 0;
  return negate ? -doubled : doubled;
}
```

### После

```typescript
function association(a: number, b: number, c: number, skip: boolean, negate: boolean) {
  const total = a + (b + c);
  const doubled = total * 2;

  if (skip) return 0;
  return negate ? -doubled : doubled;
}
```

### Ожидаемое поведение

Структура вычисления меняется при !skip, при skip возврат 0 прежний. Алгебраическая ассоциативность не доказывает равенство JS number: при [1e16,-1e16,1,false,false] результат 2 -> 0. При [1,2,3,false,false] значения совпадают, хотя data flow изменился.

### Команда

Рабочий каталог: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/08-reassociation`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function association --format text --evidence-out full.json
```

### Вывод команды — stdout без изменений

```text
Function result: association
Edit attribution uncertain
Changed computation under !skip && !negate:
  (((a + b) + c) * 2) -> ((a + (b + c)) * 2) (Changed)
  Reaches return negate ? -doubled : doubled
Before contributing writes: snippet.ts:2, snippet.ts:3
After contributing writes: snippet.ts:2, snippet.ts:3
Edit attribution uncertain
Changed computation under !skip && negate:
  (-(((a + b) + c) * 2)) -> (-((a + (b + c)) * 2)) (Changed)
  Reaches return negate ? -doubled : doubled
Before contributing writes: snippet.ts:2, snippet.ts:3
After contributing writes: snippet.ts:2, snippet.ts:3
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-6558469aee1be67a)
```

### Проверка и возможные проблемы

Обе ветки !skip правильно помечены Changed, несмотря на одинаковый набор входов. Сценарий [1e16,-1e16,1,false,false] действительно даёт 2 -> 0. На [1,2,3,false,false] значения совпадают; это согласуется со структурным смыслом Changed.

- Подтверждённый пропуск в коротком тексте: skip с возвратом 0 отсутствует, хотя comparison.regions[2] устанавливает Equal и value_dependency_changed=false.
- Два повторных сообщения о неясной атрибуции относятся к одному редактированию скобок на строке 2. Можно улучшить сопоставление перегруппировки дерева и показать общую область изменения !skip.
- Числовой свидетель не является доказательством неравенства для всех входов. Отчёт правильно сохраняет Changed, а не универсальное Different.

### Конкретные контрольные сценарии

| Аргументы по порядку параметров | До | После |
|---|---|---|
| `[10000000000000000, -10000000000000000, 1, false, false]` | `2` | `0` |
| `[1, 2, 3, false, false]` | `12` | `12` |
| `[1, 2, 3, false, true]` | `-12` | `-12` |
| `[1, 2, 3, true, false]` | `0` | `0` |

<a id="9-09-loop-boundary"></a>

## 9. Накопление в цикле и ветка нулевых итераций

Артефакты: [before.ts](09-loop-boundary/before.ts), [after.ts](09-loop-boundary/after.ts), [report.txt](09-loop-boundary/report.txt), [full.json](09-loop-boundary/full.json), [stderr.log](09-loop-boundary/stderr.log), [run.json](09-loop-boundary/run.json).

### До

```typescript
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
```

### После

```typescript
function accumulate(count: number, step: number) {
  let remaining = count;
  let total = 0;
  const increment = step + 2;

  while (remaining > 0) {
    total = total + increment;
    remaining = remaining - 1;
  }

  return total;
}
```

### Ожидаемое поведение

Для положительного конечного целого count нормальный числовой результат меняется; при count=0 результат 0. Циклы исключены из первого function-result subset: нужен явный Unknown/Unsupported, а не заявление об общей неизменности или завершении цикла для всех unrestricted-входов.

### Команда

Рабочий каталог: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/09-loop-boundary`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function accumulate --format text --evidence-out full.json
```

### Вывод команды — stdout без изменений

```text
Function result: accumulate
Edit attribution uncertain
Unknown under always: normal completion or result selection is unresolved
Coverage: result comparison Partial; source alignment Partial; before analysis Partial; after analysis Partial
Evidence: full.json (ifds-function-3a2566e347420eeb)
```

### Проверка и возможные проблемы

Команда честно возвращает Unknown и Partial для обеих сторон. Нормальных наблюдений нет; полный JSON содержит unsupported while_statement на строках 6–9. Циклы исключены из текущего function-result subset.

- Это ожидаемое ограничение поддержки, а не подтверждённая ошибка сравнения. Контрольные конечные выполнения показывают различие при положительном count, но не доказывают завершение для произвольного unrestricted-входа.
- Текстовая причина Unknown опять слишком общая: не говорится, что препятствие — неподдерживаемый while. Полный evidence содержит точную причину, которую стоит включить в короткий отчёт.

### Конкретные контрольные сценарии

| Аргументы по порядку параметров | До | После |
|---|---|---|
| `[0, 3]` | `0` | `0` |
| `[1, 3]` | `4` | `5` |
| `[2, 3]` | `8` | `10` |
| `[3, 0]` | `3` | `6` |

<a id="10-10-unused-nested-function"></a>

## 10. Изменение невызванной вложенной функции с захватом входов

Артефакты: [before.ts](10-unused-nested-function/before.ts), [after.ts](10-unused-nested-function/after.ts), [report.txt](10-unused-nested-function/report.txt), [full.json](10-unused-nested-function/full.json), [stderr.log](10-unused-nested-function/stderr.log), [run.json](10-unused-nested-function/run.json).

### До

```typescript
function outer(a: number, b: number, flag: boolean) {
  function helper(value: number) {
    let candidate = value + a;
    if (flag) candidate = candidate + b;
    return candidate * 2;
  }

  let current = a - b;
  const saved = current;
  current = b - a;
  if (flag) return saved;
  return current;
}
```

### После

```typescript
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
```

### Ожидаемое поведение

helper не вызывается; его возвраты и изменённое вычисление не должны попадать в результат outer. Data flow outer не меняется на всех входах с установленным нормальным возвратом; глобальная source alignment может остаться Partial из-за изменения файла.

### Команда

Рабочий каталог: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/10-unused-nested-function`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function outer --format text --evidence-out full.json
```

### Вывод команды — stdout без изменений

```text
Function result: outer
Unchanged data flow under always (runtime value equality unresolved)
Coverage: result comparison Partial; source alignment Partial
Evidence: full.json (ifds-function-116ab92915820665)
```

### Проверка и возможные проблемы

Изменённое тело helper не повлияло на наблюдения outer: helper не вызывается. Команда правильно сообщает неизменный data flow на всей общей области, отдельно сохраняя неизвестность runtime-равенства арифметики.

- Явной ошибки в проверенных регионах не найдено. Partial source alignment допустим: файл изменился, хотя изменение не достигает возврата outer.
- При желании можно пояснять, что diff находится в невызванной вложенной функции. Сейчас общая строка неизменности не объясняет, почему конкретное изменение исключено из результата outer.

### Конкретные контрольные сценарии

| Аргументы по порядку параметров | До | После |
|---|---|---|
| `[5, 2, false]` | `-3` | `-3` |
| `[5, 2, true]` | `3` | `3` |
| `[2, 5, false]` | `3` | `3` |
| `[2, 5, true]` | `-3` | `-3` |

## Проверка артефактов и повторный запуск

Проверено 47 сочетаний truthiness по полным структурированным регионам. Это проверка покрытия и ожидаемых признаков изменения на поддерживаемой булевой разбивке; она не устраняет неизвестность nullish, вызовов, циклов или runtime-равенства.

Приоритет исправлений: сначала восстановить единственные Equal-ветки в коротком тексте; затем раскрывать конкретные причины Unknown; после этого улучшать атрибуцию перестановок и условных источников, сохраняя роли операндов и версии записей. Поддержка циклов и более точная модель nullish — отдельные изменения анализа.

Команды записаны в [runs.json](runs.json); исходники и ожидания — в [cases.json](cases.json); результаты конкретных выполнений — в [runtime-checks.json](runtime-checks.json); сводка проверки — в [verification.json](verification.json); состояние и хеши исходников анализатора — в [engine-state.json](engine-state.json).

```sh
python3 spec/function-result-review/complex-batch-2026-10-03/run.py
python3 spec/function-result-review/complex-batch-2026-10-03/build_report.py
```

Для проверки только конкретных JS-сценариев без повторного сравнения:

```sh
python3 spec/function-result-review/complex-batch-2026-10-03/run.py --verify-only
```

CLI защищает существующий evidence: если будущий анализатор создаст другой full.json, повторный запуск сравнения завершится ошибкой. Для нового ревью используйте свежий каталог, сохранив этот снимок результатов. Скрипт run.py сохраняет stdout/stderr каждого нового запуска; архив перед повторным запуском следует скопировать целиком.
