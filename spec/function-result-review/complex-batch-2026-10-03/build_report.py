#!/usr/bin/env python3
"""Assemble the review without changing captured CLI stdout or evidence."""
import hashlib
import itertools
import json
from pathlib import Path
import shlex
import subprocess

BATCH = Path(__file__).resolve().parent
ROOT = BATCH.parents[2]

REVIEWS = {
    '01-captured-ternary': (
        'Ожидаемая область изменения и сохранение старой версии current показаны правильно. Общая изменённая цепочка saved выведена один раз; условия override сохранены отдельно.',
        ['Явной ошибки data flow в проверенных регионах не найдено.',
         'Partial относится к сравнению runtime-значений в неизменном вычислении a - b. Оно не означает, что область изменённого data flow осталась неизвестной. Для пользователя это всё ещё не очень очевидно из общей строки Coverage.']),
    '02-killed-edit': (
        'Изменённое вычисление и unused не достигают возврата. Команда правильно сообщает неизменность нормальных результатов и Complete для сравнения.',
        ['Явной ошибки в проверенных регионах не найдено. Это полезный отрицательный контроль: отличие исходного текста само по себе не является изменением возвращаемого data flow.',
         'Короткий отчёт не раскрывает правило second ? 10 : 20. Это возможное улучшение полноты объяснения, а не неправильное сравнение.']),
    '03-guard-version': (
        'Изменённые регионы first && second и !first && !second определены правильно. Неизменные регионы собраны в точную общую область (!first && second) || (first && !second). Сохранённый original не смешан с новой версией decision.',
        ['Недостаточно конкретное объяснение изменения управления: Control changed (guard correspondence unresolved) не показывает установленную зависимость decision: !first -> second. Оба источника можно проследить в full.json, но текущая текстовая политика умеет конкретно объяснять более узкий класс изменений guard.',
         'Edit attribution uncertain повторяется дважды. Для человека причина одна — запись decision на строке 4; для анализатора сопоставление редактирования источника пока не доказано. Это возможное улучшение атрибуции и группировки, а не основание объявлять её доказанной без проверки.']),
    '04-repeated-swapped-operands': (
        'Все четыре региона правильно помечены Changed. В деревьях сохранены порядок операндов, повторное использование delta и смена знака. Контроль a=b=2 даёт 0 -> 0: Changed здесь означает изменение вычисления, а не обязательное неравенство значений.',
        ['Повторное раскрытие delta создаёт четыре длинных блока и четыре Edit attribution uncertain, хотя пользователь изменил одну запись на строке 9.',
         'Атрибуция перестановки операндов остаётся неясной: текущая проверка уникального изменения не объединяет несколько изменённых чтений внутри одного переставленного выражения. Полезно распознавать такую перестановку с сохранением версий и ролей операндов.',
         'Можно добавить общую область Changed: always и одно объяснение delta, оставив конкретные вычисления каждой ветки в подробном режиме. Это предложение по отображению, не уже установленная общая формула возвращаемого значения.']),
    '05-skipped-call': (
        'При !enabled вызов действительно пропускается; неизменный data flow a + b показан. При enabled команда сохраняет Unknown и не приписывает вызову доказанное изменение возвращённого значения. Даже при !chooseCaptured неизвестный вызов уже выполнялся.',
        ['Причина Unknown слишком общая: normal completion or result selection is unresolved. В полном evidence есть unsupported call_expression на строке 2 и по два наблюдения с unknown_completion_before_return=true на каждой стороне. Пользовательский текст теряет указание на неизвестный вызов.',
         'Это корректная консервативность результата при недостаточном объяснении причины. Числовая модель mystery(x)=x*3 в контрольных сценариях не передавалась CLI и не даёт права заменить Unknown на Changed/Equal.']),
    '06-nested-selectors': (
        'Восемь изменённых регионов enabled с разными first/second/negate раскрыты правильно. Полный JSON также устанавливает Equal для !enabled с возвратом 0.',
        ['Подтверждённый пропуск в коротком тексте: ветка !enabled вообще не показана. В comparison.regions[0] есть Equal, value_dependency_changed=false и условие {2:false}; это известная неизменная ветка, а не Unknown.',
         'Порог группировки неизменных регионов требует минимум два региона. Единственный Equal без control_changed затем не попадает ни в summary, ни в обычный цикл эффектов. Поэтому успешное сравнение само по себе не гарантирует полноту короткого отчёта.',
         'Одна замена источников в const right на строке 5 порождает восемь сообщений Edit attribution uncertain и восемь одинаковых списков строк записей. Нужны более точная атрибуция условного выбора и общая область изменения enabled.',
         'Не следует автоматически упрощать a - a до 0 на unrestricted-входах. Даже на обычных числовых примерах -(a - a) даёт -0; отрицательный ноль отдельно сохранён в runtime-checks.json.']),
    '07-nullish-and-overwrite': (
        'На truthy a команда правильно показывает изменение saved при useSaved. На falsy a сохраняется Unknown: одной truthiness недостаточно, чтобы отличить 0 от null. Контрольные сценарии подтверждают, что 0 выбирает a, а null выбирает b.',
        ['Подтверждённый пропуск в коротком тексте: установленный Equal для a && !useSaved отсутствует. Он есть в comparison.regions[1], и обе стороны возвращают 100.',
         'Потеря точности при !a: Unknown охватывает и !useSaved, хотя выбранная копия уже перезаписана числом 100. Для этого случая можно отдельно исследовать объединение одинаковых нормальных результатов из нескольких вариантов nullish-выбора; нельзя просто считать falsy равным nullish.',
         'Изменение data flow при a=0 и useSaved видно при непосредственном выполнении (1 -> 2), но CLI не устанавливает соответствующую ветку. Это консервативная неполнота модели входных условий, а не ложное утверждение о равенстве.',
         'Запись условия !a в отчёте означает falsy a, а не a == null. Пояснение Unknown не раскрывает эту границу, поэтому возможна неверная интерпретация пользователем.']),
    '08-reassociation': (
        'Обе ветки !skip правильно помечены Changed, несмотря на одинаковый набор входов. Сценарий [1e16,-1e16,1,false,false] действительно даёт 2 -> 0. На [1,2,3,false,false] значения совпадают; это согласуется со структурным смыслом Changed.',
        ['Подтверждённый пропуск в коротком тексте: skip с возвратом 0 отсутствует, хотя comparison.regions[2] устанавливает Equal и value_dependency_changed=false.',
         'Два повторных сообщения о неясной атрибуции относятся к одному редактированию скобок на строке 2. Можно улучшить сопоставление перегруппировки дерева и показать общую область изменения !skip.',
         'Числовой свидетель не является доказательством неравенства для всех входов. Отчёт правильно сохраняет Changed, а не универсальное Different.']),
    '09-loop-boundary': (
        'Команда честно возвращает Unknown и Partial для обеих сторон. Нормальных наблюдений нет; полный JSON содержит unsupported while_statement на строках 6–9. Циклы исключены из текущего function-result subset.',
        ['Это ожидаемое ограничение поддержки, а не подтверждённая ошибка сравнения. Контрольные конечные выполнения показывают различие при положительном count, но не доказывают завершение для произвольного unrestricted-входа.',
         'Текстовая причина Unknown опять слишком общая: не говорится, что препятствие — неподдерживаемый while. Полный evidence содержит точную причину, которую стоит включить в короткий отчёт.']),
    '10-unused-nested-function': (
        'Изменённое тело helper не повлияло на наблюдения outer: helper не вызывается. Команда правильно сообщает неизменный data flow на всей общей области, отдельно сохраняя неизвестность runtime-равенства арифметики.',
        ['Явной ошибки в проверенных регионах не найдено. Partial source alignment допустим: файл изменился, хотя изменение не достигает возврата outer.',
         'При желании можно пояснять, что diff находится в невызванной вложенной функции. Сейчас общая строка неизменности не объясняет, почему конкретное изменение исключено из результата outer.']),
}


def oracle(slug, bits):
    if slug.startswith('01-'):
        return ('changed', True) if bits[2] and bits[3] else (None, False)
    if slug.startswith('02-'):
        return 'equal', False
    if slug.startswith('03-'):
        return ('changed', True) if bits[2] == bits[3] else ('unknown', False)
    if slug.startswith('04-'):
        return 'changed', True
    if slug.startswith('05-'):
        return 'unknown', False
    if slug.startswith('06-'):
        return ('changed', True) if bits[2] else ('equal', False)
    if slug.startswith('07-'):
        if not bits[0]:
            return 'unknown', False
        return ('changed', True) if bits[2] else ('equal', False)
    if slug.startswith('08-'):
        return ('equal', False) if bits[3] else ('changed', True)
    return 'unknown', False


def render_value(value):
    if isinstance(value, dict) and 'special' in value:
        return value['special']
    return json.dumps(value, ensure_ascii=False)


def main():
    cases = json.loads((BATCH / 'cases.json').read_text())
    runs = {run['slug']: run for run in json.loads((BATCH / 'runs.json').read_text())}
    runtime = {item['slug']: item for item in json.loads((BATCH / 'runtime-checks.json').read_text())}
    assert len(cases) == len(runs) == len(REVIEWS) == 10
    verification = dict(commands=10, successful_commands=0, runtime_samples=0,
                        boolean_assignments_checked=0, cases=[])
    parts = ['# Сравнение возвратного data flow: 10 сложных примеров\n\n',
             'Дата отчёта: 2026-10-03. Проверено текущее рабочее дерево с предыдущими изменениями анализатора и текстового отчёта. В рамках этого прогона код анализатора не исправлялся.\n\n',
             'В каждом разделе приведены полный исходный код до и после, фактически запущенная команда, её stdout без изменений, независимое ожидаемое поведение и возможные проблемы. `report.txt` хранит оригинальный stdout, `stderr.log` — полный stderr Cargo, `run.json` — аргументы, рабочий каталог и exit code, `full.json` — исходный evidence. Строка `Evidence: full.json` относится к каталогу соответствующего примера.\n\n',
             'По умолчанию CLI получает unrestricted positional inputs: аннотации TypeScript не превращаются в runtime-ограничения. `Changed` означает изменение разрешённого вычисления/источников; он не требует доказательства неравенства значений. `Unknown` из-за runtime-типа может сосуществовать с установленной неизменностью data flow. Сравниваются нормальные возвраты, а не полная эквивалентность всех эффектов программы.\n\n',
             'Все **10 команд завершились с exit code 0**. Дополнительно выполнено **45 конкретных сценариев JavaScript**; их результаты совпали с независимо заданными ожидаемыми значениями. Это конечные контрольные примеры, не доказательство для всех возможных входов. Для примера 5 контрольный runtime отдельно задаёт `mystery(x)=x*3`; CLI эту модель не получал. Для примера 7 `null` намеренно передан вопреки аннотации `number`: это допустимый unrestricted-вход анализа. `-0` сохранён отдельно от `0`.\n\n',
             '## Наблюдения, требующие внимания\n\n',
             '1. **Известная единственная неизменная ветка пропадает из короткого текста** — примеры 6, 7 и 8. В полном JSON соответствующие `Equal`-регионы есть. Это подтверждённая проблема отображения.\n',
             '2. **Атрибуция и группировка одного редактирования остаются недостаточно точными** — примеры 3, 4, 6 и 8. Особенно заметны четыре или восемь повторов `Edit attribution uncertain`. Это возможность улучшения; неопределённую атрибуцию нельзя просто объявить доказанной.\n',
             '3. **Причина Unknown теряет конкретное препятствие** — неизвестный вызов в примере 5 и неподдерживаемый цикл в примере 9. В полном JSON эта информация сохранена.\n',
             '4. **`??` теряет точность на falsy-входах** — пример 7. Нельзя подменять nullish проверкой truthiness; отдельно стоит изучить одинаковый независимый возврат 100 после перезаписи.\n',
             '5. **Одна строка Coverage смешивает впечатление о полноте data flow с runtime-доказательствами** — например, 1 и 10. Partial там не опровергает установленную область изменения/неизменности data flow.\n\n',
             '## Сводка\n\n',
             '| № | Проверка | Результат проверки | Что стоит улучшить |\n|---|---|---|---|\n']
    summaries = [
        ('Сохранённая копия и перезаписи', 'Область изменения правильная', 'Пояснение Coverage'),
        ('Уничтоженная запись', 'Изменение правильно исключено', 'Раскрытие правила выбора 10/20'),
        ('Версия guard', 'Области правильные', 'Причина control и атрибуция'),
        ('Повторное чтение и перестановка', 'Все четыре изменения найдены', '4 повтора и атрибуция'),
        ('Пропущенный/выполненный вызов', 'Консервативный Unknown обоснован', 'Указать неизвестный вызов'),
        ('Вложенные селекторы', '8 изменений найдены', 'Пропущен !enabled; 8 повторов'),
        ('Nullish и перезапись', 'Изменение видно на truthy a', 'Пропуск Equal; точность на falsy a'),
        ('Перегруппировка арифметики', 'Изменения правильно найдены', 'Пропущен skip; атрибуция'),
        ('Цикл', 'Ожидаемый Unknown', 'Указать неподдерживаемый while'),
        ('Невызванная вложенная функция', 'Изменение правильно исключено', 'Объяснить исключение diff'),
    ]
    for index, (item, summary) in enumerate(zip(cases, summaries), 1):
        parts.append(f'| [{index}](#{index}-{item["slug"]}) | {summary[0]} | {summary[1]} | {summary[2]} |\n')
    for index, item in enumerate(cases, 1):
        slug = item['slug']
        folder = BATCH / slug
        run = runs[slug]
        assert run['exit_code'] == 0
        verification['successful_commands'] += 1
        source = {side: (folder / (side + '.ts')).read_text() for side in ('before', 'after')}
        assert all(source[side] == item[side] for side in source)
        stdout = (folder / 'report.txt').read_text()
        full = json.loads((folder / 'full.json').read_text())
        comparison = full['comparison']
        slots = sorted({int(slot) for region in comparison['regions'] for slot in region['region']['values']})
        checked = 0
        for assignment in itertools.product((False, True), repeat=len(slots)):
            bits = dict(zip(slots, assignment))
            matching = [region for region in comparison['regions'] if all(bits[int(slot)] == value for slot, value in region['region']['values'].items())]
            assert len(matching) == 1, (slug, bits, len(matching))
            expected_assessment, expected_change = oracle(slug, bits)
            region = matching[0]
            assert region['value_dependency_changed'] == expected_change, (slug, bits, region)
            if expected_assessment is not None:
                assert region['assessment'] == expected_assessment, (slug, bits, region)
            checked += 1
        verification['boolean_assignments_checked'] += checked
        samples = runtime[slug]['samples']
        assert all(sample['before'] == sample['expected_before'] and sample['after'] == sample['expected_after'] for sample in samples)
        verification['runtime_samples'] += len(samples)
        record = dict(slug=slug, exit_code=run['exit_code'], result_comparison=comparison['comparison_coverage'],
                      regions=len(comparison['regions']), boolean_assignments_checked=checked,
                      runtime_samples=len(samples), text_bytes=len(stdout.encode()),
                      attribution_uncertain_mentions=stdout.count('Edit attribution uncertain'))
        verification['cases'].append(record)
        parts.extend([f'\n<a id="{index}-{slug}"></a>\n\n## {index}. {item["title"]}\n\n',
                      f'Артефакты: [before.ts]({slug}/before.ts), [after.ts]({slug}/after.ts), [report.txt]({slug}/report.txt), [full.json]({slug}/full.json), [stderr.log]({slug}/stderr.log), [run.json]({slug}/run.json).\n\n',
                      '### До\n\n```typescript\n', source['before'], '```\n\n',
                      '### После\n\n```typescript\n', source['after'], '```\n\n',
                      '### Ожидаемое поведение\n\n', item['expectation'], '\n\n',
                      '### Команда\n\n', 'Рабочий каталог: `' + str(folder) + '`. Exit code: **0**.\n\n```sh\n',
                      shlex.join(run['command']), '\n```\n\n',
                      '### Вывод команды — stdout без изменений\n\n```text\n', stdout, '```\n\n',
                      '### Проверка и возможные проблемы\n\n', REVIEWS[slug][0], '\n\n'])
        parts.extend('- ' + problem + '\n' for problem in REVIEWS[slug][1])
        parts.append('\n### Конкретные контрольные сценарии\n\n| Аргументы по порядку параметров | До | После |\n|---|---|---|\n')
        parts.extend(f'| `{json.dumps(sample["args"], ensure_ascii=False)}` | `{render_value(sample["before"])}` | `{render_value(sample["after"])}` |\n' for sample in samples)
    parts.extend(['\n## Проверка артефактов и повторный запуск\n\n',
                  f'Проверено {verification["boolean_assignments_checked"]} сочетаний truthiness по полным структурированным регионам. Это проверка покрытия и ожидаемых признаков изменения на поддерживаемой булевой разбивке; она не устраняет неизвестность nullish, вызовов, циклов или runtime-равенства.\n\n',
                  'Приоритет исправлений: сначала восстановить единственные Equal-ветки в коротком тексте; затем раскрывать конкретные причины Unknown; после этого улучшать атрибуцию перестановок и условных источников, сохраняя роли операндов и версии записей. Поддержка циклов и более точная модель nullish — отдельные изменения анализа.\n\n',
                  'Команды записаны в [runs.json](runs.json); исходники и ожидания — в [cases.json](cases.json); результаты конкретных выполнений — в [runtime-checks.json](runtime-checks.json); сводка проверки — в [verification.json](verification.json); состояние и хеши исходников анализатора — в [engine-state.json](engine-state.json).\n\n',
                  '```sh\npython3 spec/function-result-review/complex-batch-2026-10-03/run.py\npython3 spec/function-result-review/complex-batch-2026-10-03/build_report.py\n```\n\n',
                  'Для проверки только конкретных JS-сценариев без повторного сравнения:\n\n```sh\npython3 spec/function-result-review/complex-batch-2026-10-03/run.py --verify-only\n```\n\n',
                  'CLI защищает существующий evidence: если будущий анализатор создаст другой full.json, повторный запуск сравнения завершится ошибкой. Для нового ревью используйте свежий каталог, сохранив этот снимок результатов. Скрипт run.py сохраняет stdout/stderr каждого нового запуска; архив перед повторным запуском следует скопировать целиком.\n'])
    report = ''.join(parts)
    # Exact captured stdout is embedded once per case, including its final LF.
    for item in cases:
        stdout = (BATCH / item['slug'] / 'report.txt').read_text()
        assert report.count('```text\n' + stdout + '```') == 1, item['slug']
    state = dict(base_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                 working_tree=subprocess.check_output(['git', 'status', '--short'], cwd=ROOT, text=True),
                 node_version=subprocess.check_output(['node', '--version'], text=True).strip(),
                 engine_sha256={str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
                                for path in sorted((ROOT / 'src').rglob('*.rs'))},
                 dependency_lock_sha256=hashlib.sha256((ROOT / 'Cargo.lock').read_bytes()).hexdigest())
    (BATCH / 'engine-state.json').write_text(json.dumps(state, ensure_ascii=False, indent=2) + '\n')
    (BATCH / 'verification.json').write_text(json.dumps(verification, ensure_ascii=False, indent=2) + '\n')
    (BATCH / 'README.md').write_text(report)
    print(json.dumps({key: value for key, value in verification.items() if key != 'cases'}, ensure_ascii=False))
    print('Report:', BATCH / 'README.md')


if __name__ == '__main__':
    main()
