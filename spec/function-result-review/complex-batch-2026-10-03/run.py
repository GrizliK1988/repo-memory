#!/usr/bin/env python3
"""Run ten archived CLI comparisons and independent concrete JS checks."""
import json
from pathlib import Path
import argparse
import subprocess

BATCH = Path(__file__).resolve().parent
ROOT = BATCH.parents[2]


def case(slug, title, name, source, old, new, expectation, samples):
    assert source.count(old) == 1, (slug, old)
    return dict(slug=slug, title=title, function=name, before=source,
                after=source.replace(old, new), expectation=expectation,
                samples=[dict(args=args, before=before, after=after)
                         for args, before, after in samples])


CASES = [
    case("01-captured-ternary", "Сохранённое вычисление, перезаписи и ранний выход", "archive", """function archive(a: number, b: number, enabled: boolean, useSaved: boolean, override: boolean) {
  if (!enabled) return 0;

  let current = a + b;
  const saved = current * 3;

  current = a - b;
  if (override) current = 100;

  return useSaved ? saved + current : current;
}
""", "let current = a + b;", "let current = a - b;",
         "Data flow меняется только при enabled && useSaved. Сохранённая копия переживает перезаписи current; override влияет на окружающее вычисление, но не на область изменения.",
         [([5, 2, False, True, True], 0, 0), ([5, 2, True, False, False], 3, 3),
          ([5, 2, True, False, True], 100, 100), ([5, 2, True, True, False], 24, 12),
          ([5, 2, True, True, True], 121, 109)]),
    case("02-killed-edit", "Изменённая запись уничтожена двумя уровнями присваиваний", "killed", """function killed(a: number, b: number, first: boolean, second: boolean) {
  let value = a + b;
  const unused = value * 2;

  if (first) value = a;
  else value = b;

  if (second) value = 10;
  else value = 20;

  const copy = value;
  return copy;
}
""", "let value = a + b;", "let value = a - b;",
         "Изменение не достигает возврата ни в одной ветке: second выбирает 10 или 20. Сравниваются нормальные возвраты, а не все возможные эффекты выполнения арифметики.",
         [([5, 2, False, False], 20, 20), ([5, 2, False, True], 10, 10),
          ([5, 2, True, False], 20, 20), ([5, 2, True, True], 10, 10)]),
    case("03-guard-version", "Снимок условия и новая версия изменяемого guard", "versioned", """function versioned(a: number, b: number, first: boolean, second: boolean) {
  let decision = first;
  const original = decision;
  decision = !decision;

  let value = a;
  if (original) value = a + b;
  if (decision) value = b - a;

  return value;
}
""", "decision = !decision;", "decision = second;",
         "До изменения decision равен !first, после — second. Data flow меняется при (!first && !second) || (first && second); original остаётся снимком first.",
         [([5, 2, False, False], -3, 5), ([5, 2, False, True], -3, -3),
          ([5, 2, True, False], 7, 7), ([5, 2, True, True], 7, -3)]),
    case("04-repeated-swapped-operands", "Перестановка операндов через копии и повторное чтение", "repeated", """function repeated(a: number, b: number, reverse: boolean, negate: boolean) {
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
""", "const delta = left - right;", "const delta = right - left;",
         "Data flow меняется во всех четырёх сочетаниях reverse/negate: важны роли операндов, а не набор источников {a,b}. Повторное чтение delta не должно смешивать разные версии записи. Изменение не доказывает неравенство значений при любых входах.",
         [([5, 2, False, False], 6, -6), ([5, 2, True, False], -6, 6),
          ([5, 2, False, True], -6, 6), ([5, 2, True, True], 6, -6),
          ([2, 2, False, False], 0, 0)]),
    case("05-skipped-call", "Короткое замыкание и выполненный неизвестный вызов", "skipped", """function skipped(a: number, b: number, enabled: boolean, chooseCaptured: boolean) {
  const captured = enabled && mystery(a);
  if (!enabled) return a + b;

  const fallback = b - a;
  return chooseCaptured ? captured : fallback;
}
""", "mystery(a)", "mystery(b)",
         "При !enabled вызов пропущен и data flow возврата не меняется. При enabled результат/завершение неизвестны даже при !chooseCaptured: вызов уже был выполнен. Для числовых контрольных сценариев отдельно задана модель mystery(x)=x*3; CLI этой модели не получает.",
         [([5, 2, False, False], 7, 7), ([5, 2, False, True], 7, 7),
          ([5, 2, True, False], -3, -3), ([5, 2, True, True], 15, 6)]),
    case("06-nested-selectors", "Два условных источника, ранний выход и смена знака", "selectors", """function selectors(a: number, b: number, enabled: boolean, first: boolean, second: boolean, negate: boolean) {
  if (!enabled) return 0;

  const left = first ? a : b;
  const right = second ? b : a;
  const frozen = left - right;

  return negate ? -frozen : frozen;
}
""", "const right = second ? b : a;", "const right = second ? a : b;",
         "При enabled выбранный правый операнд меняется во всех сочетаниях first/second/negate. При !enabled возврат 0 не меняется. Источники после изменения могут совпасть внутри одного вычисления — это не повод потерять роли операндов.",
         [([5, 2, False, True, True, False], 0, 0),
          ([5, 2, True, True, False, False], 0, 3), ([5, 2, True, True, True, False], 3, 0),
          ([5, 2, True, False, False, False], -3, 0), ([5, 2, True, False, True, False], 0, -3),
          ([5, 2, True, True, True, True], -3, {"special": "-0"})]),
    case("07-nullish-and-overwrite", "Nullish-выбор, сохранённая копия и независимый возврат", "nullish", """function nullish(a: number, b: number, useSaved: boolean) {
  let selected = a ?? b;
  const saved = selected + 1;

  selected = 100;
  if (useSaved) return saved;
  return selected;
}
""", "const saved = selected + 1;", "const saved = selected + 2;",
         "Логически вычисление saved меняется при useSaved, а при !useSaved возвращается 100. На unrestricted-входе falsy не означает nullish: 0 должен остаться выбранным a, null должен выбрать b. Консервативный Unknown при неясном выборе — ограничение точности, а не доказательство отсутствия изменения.",
         [([5, 2, True], 6, 7), ([0, 2, True], 1, 2), ([None, 2, True], 3, 4),
          ([None, 2, False], 100, 100), ([0, 2, False], 100, 100)]),
    case("08-reassociation", "Перегруппировка арифметики с округлением и ранним возвратом", "association", """function association(a: number, b: number, c: number, skip: boolean, negate: boolean) {
  const total = (a + b) + c;
  const doubled = total * 2;

  if (skip) return 0;
  return negate ? -doubled : doubled;
}
""", "const total = (a + b) + c;", "const total = a + (b + c);",
         "Структура вычисления меняется при !skip, при skip возврат 0 прежний. Алгебраическая ассоциативность не доказывает равенство JS number: при [1e16,-1e16,1,false,false] результат 2 -> 0. При [1,2,3,false,false] значения совпадают, хотя data flow изменился.",
         [([10000000000000000, -10000000000000000, 1, False, False], 2, 0),
          ([1, 2, 3, False, False], 12, 12), ([1, 2, 3, False, True], -12, -12),
          ([1, 2, 3, True, False], 0, 0)]),
    case("09-loop-boundary", "Накопление в цикле и ветка нулевых итераций", "accumulate", """function accumulate(count: number, step: number) {
  let remaining = count;
  let total = 0;
  const increment = step + 1;

  while (remaining > 0) {
    total = total + increment;
    remaining = remaining - 1;
  }

  return total;
}
""", "const increment = step + 1;", "const increment = step + 2;",
         "Для положительного конечного целого count нормальный числовой результат меняется; при count=0 результат 0. Циклы исключены из первого function-result subset: нужен явный Unknown/Unsupported, а не заявление об общей неизменности или завершении цикла для всех unrestricted-входов.",
         [([0, 3], 0, 0), ([1, 3], 4, 5), ([2, 3], 8, 10), ([3, 0], 3, 6)]),
    case("10-unused-nested-function", "Изменение невызванной вложенной функции с захватом входов", "outer", """function outer(a: number, b: number, flag: boolean) {
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
""", "let candidate = value + a;", "let candidate = value - a;",
         "helper не вызывается; его возвраты и изменённое вычисление не должны попадать в результат outer. Data flow outer не меняется на всех входах с установленным нормальным возвратом; глобальная source alignment может остаться Partial из-за изменения файла.",
         [([5, 2, False], -3, -3), ([5, 2, True], 3, 3),
          ([2, 5, False], 3, 3), ([2, 5, True], -3, -3)]),
]

NODE_CHECK = r"""
const vm = require('node:vm');
const fs = require('node:fs');
const cases = JSON.parse(fs.readFileSync(0, 'utf8'));
function encode(value) {
  if (Object.is(value, -0)) return {special: '-0'};
  if (Number.isNaN(value)) return {special: 'NaN'};
  if (value === undefined) return {special: 'undefined'};
  if (value === Infinity || value === -Infinity) return {special: String(value)};
  return value;
}
const results = cases.map(item => ({slug: item.slug, samples: item.samples.map(sample => {
  const evaluate = source => encode(vm.runInNewContext(
    source.replace(/:\s*(number|boolean)\b/g, '') + '\n' + item.function + '(...args)',
    {args: sample.args, mystery: x => x * 3}, {timeout: 1000}));
  return {args: sample.args, before: evaluate(item.before), after: evaluate(item.after),
    expected_before: sample.before, expected_after: sample.after};
})}));
process.stdout.write(JSON.stringify(results, null, 2) + '\n');
"""


def verify_samples():
    checked = subprocess.run(['node', '-e', NODE_CHECK], input=json.dumps(CASES),
                             text=True, capture_output=True, check=True)
    (BATCH / 'runtime-checks.json').write_text(checked.stdout)
    checks = json.loads(checked.stdout)
    failures = []
    for item in checks:
        for sample in item['samples']:
            if sample['before'] != sample['expected_before'] or sample['after'] != sample['expected_after']:
                failures.append(dict(slug=item['slug'], sample=sample))
    print(f"Concrete JS samples: {sum(len(item['samples']) for item in checks)}; failures={len(failures)}", flush=True)
    if failures:
        raise AssertionError(failures)


def main(verify_only=False):
    (BATCH / 'cases.json').write_text(json.dumps(CASES, ensure_ascii=False, indent=2) + '\n')
    if verify_only:
        verify_samples()
        return
    runs = []
    for item in CASES:
        folder = BATCH / item['slug']
        folder.mkdir(exist_ok=True)
        for side in ('before', 'after'):
            (folder / (side + '.ts')).write_text(item[side])
        command = ['cargo', 'run', '--locked', '--offline', '--manifest-path',
                   str(ROOT / 'Cargo.toml'), '--example', 'ifds_compare', '--',
                   'before.ts', 'after.ts', '--function', item['function'],
                   '--format', 'text', '--evidence-out', 'full.json']
        completed = subprocess.run(command, cwd=folder, capture_output=True, text=True)
        (folder / 'report.txt').write_text(completed.stdout)
        (folder / 'stderr.log').write_text(completed.stderr)
        run = dict(slug=item['slug'], command=command, cwd=str(folder), exit_code=completed.returncode)
        (folder / 'run.json').write_text(json.dumps(run, ensure_ascii=False, indent=2) + '\n')
        runs.append(run)
        print(f"{item['slug']}: exit={completed.returncode}; stdout={len(completed.stdout)} chars", flush=True)
    (BATCH / 'runs.json').write_text(json.dumps(runs, ensure_ascii=False, indent=2) + '\n')
    verify_samples()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--verify-only', action='store_true', help='check concrete samples without rerunning the CLI')
    main(parser.parse_args().verify_only)
