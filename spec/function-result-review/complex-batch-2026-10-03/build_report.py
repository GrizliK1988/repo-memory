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
        'The expected region of change and preservation of the old current version are shown correctly. The shared changed saved chain appears once; override conditions remain separate.',
        ['No clear data-flow error was found in the checked regions.',
         'Partial refers to comparison of runtime values in the unchanged computation a - b. It does not mean that the region of changed data flow remains unknown. The combined Coverage line still does not make this clear to the user.']),
    '02-killed-edit': (
        'The changed computation and unused do not reach the return. The command correctly reports unchanged normal results and Complete comparison coverage.',
        ['No clear error was found in the checked regions. This is a useful negative control: a source-text difference alone does not constitute a change in returned data flow.',
         'The short report does not explain the second ? 10 : 20 selection rule. This could improve explanation completeness; the comparison itself is correct.']),
    '03-guard-version': (
        'The changed regions first && second and !first && !second are identified correctly. Unchanged regions form the exact combined domain (!first && second) || (first && !second). The saved original is not mixed with the new decision version.',
        ['The control-change explanation is not specific enough: Control changed (guard correspondence unresolved) does not show the established decision dependency: !first -> second. Both sources can be traced in full.json, but the current text policy specifically explains a narrower class of guard changes.',
         'Edit attribution uncertain appears twice. A reader sees one cause — the decision write on line 4; the analyzer has not yet proven correspondence of the source edit. This could improve attribution and grouping, but does not justify declaring attribution proven without verification.']),
    '04-repeated-swapped-operands': (
        'All four regions are correctly marked Changed. The trees preserve operand order, repeated use of delta, and sign reversal. The a=b=2 check gives 0 -> 0: Changed here means a changed computation, not necessarily unequal values.',
        ['Repeated expansion of delta creates four long blocks and four Edit attribution uncertain messages, although the user changed one write on line 9.',
         'Attribution of the operand swap remains unresolved: the current unique-change check does not combine multiple changed reads inside one swapped expression. Recognizing such swaps while preserving operand versions and roles would help.',
         'A combined Changed: always region and one delta explanation could be added, keeping specific computations for each branch in verbose mode. This is a presentation suggestion, not an already established common formula for the returned value.']),
    '05-skipped-call': (
        'Under !enabled, the call is indeed skipped and unchanged data flow a + b is shown. Under enabled, the command retains Unknown without claiming a proven change in the returned value. Even under !chooseCaptured, the unknown call has already executed.',
        ['The Unknown reason is too general: normal completion or result selection is unresolved. The full evidence contains unsupported call_expression on line 2 and two observations with unknown_completion_before_return=true on each side. The user-facing text loses the reference to the unknown call.',
         'The result is correctly conservative, but the reason is insufficiently explained. The numerical model mystery(x)=x*3 in the concrete checks was not supplied to the CLI and does not justify replacing Unknown with Changed/Equal.']),
    '06-nested-selectors': (
        'The eight changed enabled regions with different first/second/negate values are expanded correctly. The full JSON also establishes Equal for !enabled with return value 0.',
        ['Confirmed omission in the short text: the !enabled branch is not shown at all. comparison.regions[0] contains Equal, value_dependency_changed=false, and condition {2:false}; this is a known unchanged branch, not Unknown.',
         'The unchanged-region grouping threshold requires at least two regions. A single Equal without control_changed then appears neither in the summary nor in the regular effect loop. Successful comparison alone therefore does not guarantee a complete short report.',
         'One source replacement in const right on line 5 produces eight Edit attribution uncertain messages and eight identical write-location lists. More precise conditional-selection attribution and a combined enabled change region are needed.',
         'Do not automatically simplify a - a to 0 for unrestricted inputs. Even ordinary numerical examples produce -0 for -(a - a); negative zero is preserved separately in runtime-checks.json.']),
    '07-nullish-and-overwrite': (
        'For truthy a, the command correctly shows the saved change under useSaved. For falsy a, it retains Unknown: truthiness alone cannot distinguish 0 from null. Concrete checks confirm that 0 selects a while null selects b.',
        ['Confirmed omission in the short text: the established Equal for a && !useSaved is missing. It exists in comparison.regions[1], and both sides return 100.',
         'Precision is lost under !a: Unknown also covers !useSaved, although the selected copy has already been overwritten with 100. Combining identical normal results from several nullish-selection alternatives deserves separate investigation; falsy must not simply be treated as nullish.',
         'The data-flow change for a=0 and useSaved is visible in direct execution (1 -> 2), but the CLI does not establish that branch. This is conservative incompleteness in the input-condition model, not a false equality claim.',
         'The !a condition in the report means falsy a, not a == null. The Unknown explanation does not describe this boundary, so users may misinterpret it.']),
    '08-reassociation': (
        'Both !skip branches are correctly marked Changed despite using the same input set. The [1e16,-1e16,1,false,false] scenario indeed gives 2 -> 0. For [1,2,3,false,false], the values coincide, consistent with the structural meaning of Changed.',
        ['Confirmed omission in the short text: skip with return value 0 is missing, although comparison.regions[2] establishes Equal and value_dependency_changed=false.',
         'Two repeated uncertain-attribution messages refer to one parenthesis edit on line 2. Tree-reassociation correspondence could be improved, with a combined !skip change region.',
         'A numerical witness does not prove inequality for all inputs. The report correctly retains Changed rather than a universal Different.']),
    '09-loop-boundary': (
        'The command reports Unknown and Partial for both sides. There are no normal observations; the full JSON contains unsupported while_statement on lines 6–9. Loops are excluded from the current function-result subset.',
        ['This is an expected support limitation, not a confirmed comparison error. Concrete finite executions show a difference for positive count but do not prove termination for arbitrary unrestricted inputs.',
         'The textual Unknown reason is again too general: it does not identify the unsupported while as the obstacle. The full evidence contains the precise reason, which should be included in the short report.']),
    '10-unused-nested-function': (
        'The changed helper body does not affect outer observations: helper is never called. The command correctly reports unchanged data flow over the entire common domain while retaining uncertainty about runtime equality of arithmetic.',
        ['No clear error was found in the checked regions. Partial source alignment is acceptable: the file changed, but the change does not reach the return of outer.',
         'The report could explain that the diff is in an uncalled nested function. The current general unchanged-flow line does not explain why this particular edit is excluded from the result of outer.']),
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
    parts = ['# Comparing return data flow: 10 complex examples\n\n',
             'Report date: 2026-10-03. The current working tree was checked with the preceding analyzer and text-report changes. No analyzer code was fixed during this run.\n\n',
             'Each section includes the complete before/after source, the actual command, its unchanged stdout, independently expected behavior, and possible issues. `report.txt` stores the original stdout, `stderr.log` the full Cargo stderr, `run.json` the arguments, working directory, and exit code, and `full.json` the original evidence. The `Evidence: full.json` line refers to the directory of the corresponding example.\n\n',
             'By default, the CLI receives unrestricted positional inputs: TypeScript annotations do not become runtime constraints. `Changed` means a change in the resolved computation/sources; it does not require proof of unequal values. `Unknown` due to runtime types can coexist with established unchanged data flow. The comparison covers normal returns, not full equivalence of all program effects.\n\n',
             'All **10 commands finished with exit code 0**. In addition, **45 concrete JavaScript scenarios** were executed; their results matched independently specified expected values. These are finite checks, not a proof for all possible inputs. For example 5, the check runtime separately defines `mystery(x)=x*3`; the CLI did not receive this model. For example 7, `null` was deliberately passed despite the `number` annotation: it is a valid unrestricted analysis input. `-0` is preserved separately from `0`.\n\n',
             '## Findings requiring attention\n\n',
             '1. **A single known unchanged branch disappears from the short text** — examples 6, 7, and 8. The corresponding `Equal` regions exist in the full JSON. This is a confirmed presentation issue.\n',
             '2. **Attribution and grouping of a single edit remain insufficiently precise** — examples 3, 4, 6, and 8. Four or eight repetitions of `Edit attribution uncertain` are especially noticeable. This is an opportunity for improvement; uncertain attribution cannot simply be declared proven.\n',
             '3. **The Unknown reason loses the specific obstacle** — the unknown call in example 5 and unsupported loop in example 9. The full JSON retains this information.\n',
             '4. **`??` loses precision for falsy inputs** — example 7. Nullish selection must not be replaced by truthiness; the identical independent return of 100 after an overwrite also deserves separate investigation.\n',
             '5. **One Coverage line mixes data-flow completeness with runtime proofs** — examples 1 and 10, among others. Partial does not refute the established region of changed/unchanged data flow there.\n\n',
             '## Summary\n\n',
             '| No. | Check | Check result | Suggested improvement |\n|---|---|---|---|\n']
    summaries = [
        ('Saved copy and overwrites', 'Correct change region', 'Explain Coverage'),
        ('Killed write', 'Change correctly excluded', 'Explain the 10/20 selection rule'),
        ('Guard version', 'Correct regions', 'Control cause and attribution'),
        ('Repeated reads and operand swap', 'All four changes found', '4 repetitions and attribution'),
        ('Skipped/executed call', 'Conservative Unknown is justified', 'Identify the unknown call'),
        ('Nested selectors', '8 changes found', '!enabled omitted; 8 repetitions'),
        ('Nullish selection and overwrite', 'Change visible for truthy a', 'Equal omitted; precision for falsy a'),
        ('Arithmetic reassociation', 'Changes correctly found', 'skip omitted; attribution'),
        ('Loop', 'Expected Unknown', 'Identify the unsupported while'),
        ('Uncalled nested function', 'Change correctly excluded', 'Explain exclusion of the diff'),
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
                      f'Artifacts: [before.ts]({slug}/before.ts), [after.ts]({slug}/after.ts), [report.txt]({slug}/report.txt), [full.json]({slug}/full.json), [stderr.log]({slug}/stderr.log), [run.json]({slug}/run.json).\n\n',
                      '### Before\n\n```typescript\n', source['before'], '```\n\n',
                      '### After\n\n```typescript\n', source['after'], '```\n\n',
                      '### Expected behavior\n\n', item['expectation'], '\n\n',
                      '### Command\n\n', 'Working directory: `' + str(folder) + '`. Exit code: **0**.\n\n```sh\n',
                      shlex.join(run['command']), '\n```\n\n',
                      '### Command output — unchanged stdout\n\n```text\n', stdout, '```\n\n',
                      '### Verification and possible issues\n\n', REVIEWS[slug][0], '\n\n'])
        parts.extend('- ' + problem + '\n' for problem in REVIEWS[slug][1])
        parts.append('\n### Concrete checks\n\n| Arguments in parameter order | Before | After |\n|---|---|---|\n')
        parts.extend(f'| `{json.dumps(sample["args"], ensure_ascii=False)}` | `{render_value(sample["before"])}` | `{render_value(sample["after"])}` |\n' for sample in samples)
    parts.extend(['\n## Artifact verification and rerun\n\n',
                  f'Checked {verification["boolean_assignments_checked"]} truthiness combinations against the full structured regions. This checks coverage and expected change indicators over the supported Boolean partition; it does not eliminate uncertainty about nullish selection, calls, loops, or runtime equality.\n\n',
                  'Fix priorities: first restore single Equal branches in the short text; then explain specific Unknown reasons; afterward improve attribution of swaps and conditional sources while preserving operand roles and write versions. Loop support and a more precise nullish model are separate analysis changes.\n\n',
                  'Commands are recorded in [runs.json](runs.json); sources and expectations in [cases.json](cases.json); concrete execution results in [runtime-checks.json](runtime-checks.json); verification summary in [verification.json](verification.json); analyzer state and source hashes in [engine-state.json](engine-state.json).\n\n',
                  '```sh\npython3 spec/function-result-review/complex-batch-2026-10-03/run.py\npython3 spec/function-result-review/complex-batch-2026-10-03/build_report.py\n```\n\n',
                  'To check only the concrete JS scenarios without repeating the comparison:\n\n```sh\npython3 spec/function-result-review/complex-batch-2026-10-03/run.py --verify-only\n```\n\n',
                  'The CLI protects existing evidence: if a future analyzer produces a different full.json, rerunning the comparison will fail. For a new review, use a fresh directory and preserve this result snapshot. run.py saves stdout/stderr from each new run; copy the entire archive before rerunning it.\n'])
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
