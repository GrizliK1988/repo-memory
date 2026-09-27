# Проверка отчётов о результатах функций

Воспроизводимый набор сравнений для уже поддерживаемого синхронного подмножества: литералы, копии и перезаписи, ветви и приоритет записей, переписанные условия, ранние возвраты, `undefined` при достижении конца функции, короткое замыкание, простые выражения. Последний пример — контроль неизвестного вызова.

Каждый каталог содержит `before.ts`, `after.ts`, текстовый отчёт, компактный JSON и полный JSON с доказательствами. Компактный отчёт ссылается на соседний `full.json`; ссылки проверены. Текст и компактный JSON сохранены после запуска команды ниже. В `compact.json` путь доказательства нормализован к локальному `full.json`, в тексте абсолютный временный путь заменён на относительный. Семантические поля исходных отчётов сохранены.

Запускать из каталога конкретного примера:

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml \
  --example ifds_compare -- before.ts after.ts --function result \
  --format text --evidence-out full.json
```

Для compact/full JSON заменить `text` на `compact-json` или `full-json`. Команда full-json печатает полное свидетельство в stdout; вариант text/compact-json записывает его в `full.json`.

| Пример | Причина или контроль | Отчёты |
| --- | --- | --- |
| `literal` | Замена литерала | [`report.txt`](reports/literal/report.txt) · [`compact.json`](reports/literal/compact.json) · [`full.json`](reports/literal/full.json) |
| `copy` | Изменение источника сохранённой копии | [`report.txt`](reports/copy/report.txt) · [`compact.json`](reports/copy/compact.json) · [`full.json`](reports/copy/full.json) |
| `priority` | Перестановка условных записей | [`report.txt`](reports/priority/report.txt) · [`compact.json`](reports/priority/compact.json) · [`full.json`](reports/priority/full.json) |
| `guard` | Дополнительное условие после ранних возвратов | [`report.txt`](reports/guard/report.txt) · [`compact.json`](reports/guard/compact.json) · [`full.json`](reports/guard/full.json) |
| `guard_version` | Перезапись значения условия | [`report.txt`](reports/guard_version/report.txt) · [`compact.json`](reports/guard_version/compact.json) · [`full.json`](reports/guard_version/full.json) |
| `early_return` | Добавление раннего возврата | [`report.txt`](reports/early_return/report.txt) · [`compact.json`](reports/early_return/compact.json) · [`full.json`](reports/early_return/full.json) |
| `fallthrough` | Замена неявного undefined | [`report.txt`](reports/fallthrough/report.txt) · [`compact.json`](reports/fallthrough/compact.json) · [`full.json`](reports/fallthrough/full.json) |
| `short_circuit` | Замена результата правой ветки && | [`report.txt`](reports/short_circuit/report.txt) · [`compact.json`](reports/short_circuit/compact.json) · [`full.json`](reports/short_circuit/full.json) |
| `expression` | Изменение вычисления | [`report.txt`](reports/expression/report.txt) · [`compact.json`](reports/expression/compact.json) · [`full.json`](reports/expression/full.json) |
| `overwritten` | Контроль: изменённая запись убита | [`report.txt`](reports/overwritten/report.txt) · [`compact.json`](reports/overwritten/compact.json) · [`full.json`](reports/overwritten/full.json) |
| `equal_control` | Контроль: новое условие с тем же результатом | [`report.txt`](reports/equal_control/report.txt) · [`compact.json`](reports/equal_control/compact.json) · [`full.json`](reports/equal_control/full.json) |
| `unknown` | Контроль: неподдержанный вызов в одной ветке | [`report.txt`](reports/unknown/report.txt) · [`compact.json`](reports/unknown/compact.json) · [`full.json`](reports/unknown/full.json) |
| `known_expression` | Изменение вычисления с известным числовым источником | [`report.txt`](reports/known_expression/report.txt) · [`compact.json`](reports/known_expression/compact.json) · [`full.json`](reports/known_expression/full.json) |

Семантические ожидания по каждому примеру и исходники собраны в [EXAMPLES.md](EXAMPLES.md). Проверка и найденные вопросы описаны в отдельных файлах [001](001-report-values-and-attribution.md), [002](002-short-circuit-precision.md) и [003](003-compact-readable-text.md).

Проверка схем и ссылок, truth tables, результаты тестов и размеры файлов зафиксированы в [verification.json](reports/verification.json). Отчёты не являются golden-эталонами реализации: ожидаемые результаты задавались отдельно по исходникам.
