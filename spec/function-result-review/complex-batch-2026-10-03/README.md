# Comparing return data flow: 10 complex examples

Current status after fixing example 3 and rerunning all 10 commands:
[follow-up on attribution and remaining work](FOLLOW_UP.md). The original report
from the previous run is preserved below.

Report date: 2026-10-03. The current working tree was checked with the preceding analyzer and text-report changes. No analyzer code was fixed during this run.

Each section includes the complete before/after source, the actual command, its unchanged stdout, independently expected behavior, and possible issues. `report.txt` stores the original stdout, `stderr.log` the full Cargo stderr, `run.json` the arguments, working directory, and exit code, and `full.json` the original evidence. The `Evidence: full.json` line refers to the directory of the corresponding example.

By default, the CLI receives unrestricted positional inputs: TypeScript annotations do not become runtime constraints. `Changed` means a change in the resolved computation/sources; it does not require proof of unequal values. `Unknown` due to runtime types can coexist with established unchanged data flow. The comparison covers normal returns, not full equivalence of all program effects.

All **10 commands finished with exit code 0**. In addition, **45 concrete JavaScript scenarios** were executed; their results matched independently specified expected values. These are finite checks, not a proof for all possible inputs. For example 5, the check runtime separately defines `mystery(x)=x*3`; the CLI did not receive this model. For example 7, `null` was deliberately passed despite the `number` annotation: it is a valid unrestricted analysis input. `-0` is preserved separately from `0`.

## Findings requiring attention

1. **A single known unchanged branch disappears from the short text** — examples 6, 7, and 8. The corresponding `Equal` regions exist in the full JSON. This is a confirmed presentation issue.
2. **Attribution and grouping of a single edit remain insufficiently precise** — examples 3, 4, 6, and 8. Four or eight repetitions of `Edit attribution uncertain` are especially noticeable. This is an opportunity for improvement; uncertain attribution cannot simply be declared proven.
3. **The Unknown reason loses the specific obstacle** — the unknown call in example 5 and unsupported loop in example 9. The full JSON retains this information.
4. **`??` loses precision for falsy inputs** — example 7. Nullish selection must not be replaced by truthiness; the identical independent return of 100 after an overwrite also deserves separate investigation.
5. **One Coverage line mixes data-flow completeness with runtime proofs** — examples 1 and 10, among others. Partial does not refute the established region of changed/unchanged data flow there.

## Summary

| No. | Check | Check result | Suggested improvement |
|---|---|---|---|
| [1](#1-01-captured-ternary) | Saved copy and overwrites | Correct change region | Explain Coverage |
| [2](#2-02-killed-edit) | Killed write | Change correctly excluded | Explain the 10/20 selection rule |
| [3](#3-03-guard-version) | Guard version | Correct regions | Control cause and attribution |
| [4](#4-04-repeated-swapped-operands) | Repeated reads and operand swap | All four changes found | 4 repetitions and attribution |
| [5](#5-05-skipped-call) | Skipped/executed call | Conservative Unknown is justified | Identify the unknown call |
| [6](#6-06-nested-selectors) | Nested selectors | 8 changes found | !enabled omitted; 8 repetitions |
| [7](#7-07-nullish-and-overwrite) | Nullish selection and overwrite | Change visible for truthy a | Equal omitted; precision for falsy a |
| [8](#8-08-reassociation) | Arithmetic reassociation | Changes correctly found | skip omitted; attribution |
| [9](#9-09-loop-boundary) | Loop | Expected Unknown | Identify the unsupported while |
| [10](#10-10-unused-nested-function) | Uncalled nested function | Change correctly excluded | Explain exclusion of the diff |

<a id="1-01-captured-ternary"></a>

## 1. Saved computation, overwrites, and early exit

Artifacts: [before.ts](01-captured-ternary/before.ts), [after.ts](01-captured-ternary/after.ts), [report.txt](01-captured-ternary/report.txt), [full.json](01-captured-ternary/full.json), [stderr.log](01-captured-ternary/stderr.log), [run.json](01-captured-ternary/run.json).

### Before

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

### After

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

### Expected behavior

Data flow changes only under enabled && useSaved. The saved copy survives overwrites of current; override affects the surrounding computation but not the region of change.

### Command

Working directory: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/01-captured-ternary`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function archive --format text --evidence-out full.json
```

### Command output — unchanged stdout

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

### Verification and possible issues

The expected region of change and preservation of the old current version are shown correctly. The shared changed saved chain appears once; override conditions remain separate.

- No clear data-flow error was found in the checked regions.
- Partial refers to comparison of runtime values in the unchanged computation a - b. It does not mean that the region of changed data flow remains unknown. The combined Coverage line still does not make this clear to the user.

### Concrete checks

| Arguments in parameter order | Before | After |
|---|---|---|
| `[5, 2, false, true, true]` | `0` | `0` |
| `[5, 2, true, false, false]` | `3` | `3` |
| `[5, 2, true, false, true]` | `100` | `100` |
| `[5, 2, true, true, false]` | `24` | `12` |
| `[5, 2, true, true, true]` | `121` | `109` |

<a id="2-02-killed-edit"></a>

## 2. Changed write killed by two layers of assignments

Artifacts: [before.ts](02-killed-edit/before.ts), [after.ts](02-killed-edit/after.ts), [report.txt](02-killed-edit/report.txt), [full.json](02-killed-edit/full.json), [stderr.log](02-killed-edit/stderr.log), [run.json](02-killed-edit/run.json).

### Before

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

### After

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

### Expected behavior

The change reaches no return branch: second selects 10 or 20. The comparison covers normal returns, not every possible effect of arithmetic evaluation.

### Command

Working directory: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/02-killed-edit`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function killed --format text --evidence-out full.json
```

### Command output — unchanged stdout

```text
Function result: killed
Normal results unchanged on common inputs: always (under the recorded entry assumptions).
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-cdd967feba9cf6b7)
```

### Verification and possible issues

The changed computation and unused do not reach the return. The command correctly reports unchanged normal results and Complete comparison coverage.

- No clear error was found in the checked regions. This is a useful negative control: a source-text difference alone does not constitute a change in returned data flow.
- The short report does not explain the second ? 10 : 20 selection rule. This could improve explanation completeness; the comparison itself is correct.

### Concrete checks

| Arguments in parameter order | Before | After |
|---|---|---|
| `[5, 2, false, false]` | `20` | `20` |
| `[5, 2, false, true]` | `10` | `10` |
| `[5, 2, true, false]` | `20` | `20` |
| `[5, 2, true, true]` | `10` | `10` |

<a id="3-03-guard-version"></a>

## 3. Condition snapshot and a new version of a mutable guard

Artifacts: [before.ts](03-guard-version/before.ts), [after.ts](03-guard-version/after.ts), [report.txt](03-guard-version/report.txt), [full.json](03-guard-version/full.json), [stderr.log](03-guard-version/stderr.log), [run.json](03-guard-version/run.json).

### Before

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

### After

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

### Expected behavior

Before the change, decision equals !first; afterward, it equals second. Data flow changes under (!first && !second) || (first && second); original remains a snapshot of first.

### Command

Working directory: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/03-guard-version`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function versioned --format text --evidence-out full.json
```

### Command output — unchanged stdout

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

### Verification and possible issues

The changed regions first && second and !first && !second are identified correctly. Unchanged regions form the exact combined domain (!first && second) || (first && !second). The saved original is not mixed with the new decision version.

- The control-change explanation is not specific enough: Control changed (guard correspondence unresolved) does not show the established decision dependency: !first -> second. Both sources can be traced in full.json, but the current text policy specifically explains a narrower class of guard changes.
- Edit attribution uncertain appears twice. A reader sees one cause — the decision write on line 4; the analyzer has not yet proven correspondence of the source edit. This could improve attribution and grouping, but does not justify declaring attribution proven without verification.

### Concrete checks

| Arguments in parameter order | Before | After |
|---|---|---|
| `[5, 2, false, false]` | `-3` | `5` |
| `[5, 2, false, true]` | `-3` | `-3` |
| `[5, 2, true, false]` | `7` | `7` |
| `[5, 2, true, true]` | `7` | `-3` |

<a id="4-04-repeated-swapped-operands"></a>

## 4. Swapped operands through copies and repeated reads

Artifacts: [before.ts](04-repeated-swapped-operands/before.ts), [after.ts](04-repeated-swapped-operands/after.ts), [report.txt](04-repeated-swapped-operands/report.txt), [full.json](04-repeated-swapped-operands/full.json), [stderr.log](04-repeated-swapped-operands/stderr.log), [run.json](04-repeated-swapped-operands/run.json).

### Before

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

### After

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

### Expected behavior

Data flow changes in all four reverse/negate combinations: operand roles matter, not just the set of sources {a,b}. Repeated reads of delta must not mix write versions. The change does not prove unequal values for every input.

### Command

Working directory: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/04-repeated-swapped-operands`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function repeated --format text --evidence-out full.json
```

### Command output — unchanged stdout

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

### Verification and possible issues

All four regions are correctly marked Changed. The trees preserve operand order, repeated use of delta, and sign reversal. The a=b=2 check gives 0 -> 0: Changed here means a changed computation, not necessarily unequal values.

- Repeated expansion of delta creates four long blocks and four Edit attribution uncertain messages, although the user changed one write on line 9.
- Attribution of the operand swap remains unresolved: the current unique-change check does not combine multiple changed reads inside one swapped expression. Recognizing such swaps while preserving operand versions and roles would help.
- A combined Changed: always region and one delta explanation could be added, keeping specific computations for each branch in verbose mode. This is a presentation suggestion, not an already established common formula for the returned value.

### Concrete checks

| Arguments in parameter order | Before | After |
|---|---|---|
| `[5, 2, false, false]` | `6` | `-6` |
| `[5, 2, true, false]` | `-6` | `6` |
| `[5, 2, false, true]` | `-6` | `6` |
| `[5, 2, true, true]` | `6` | `-6` |
| `[2, 2, false, false]` | `0` | `0` |

<a id="5-05-skipped-call"></a>

## 5. Short-circuiting and an executed unknown call

Artifacts: [before.ts](05-skipped-call/before.ts), [after.ts](05-skipped-call/after.ts), [report.txt](05-skipped-call/report.txt), [full.json](05-skipped-call/full.json), [stderr.log](05-skipped-call/stderr.log), [run.json](05-skipped-call/run.json).

### Before

```typescript
function skipped(a: number, b: number, enabled: boolean, chooseCaptured: boolean) {
  const captured = enabled && mystery(a);
  if (!enabled) return a + b;

  const fallback = b - a;
  return chooseCaptured ? captured : fallback;
}
```

### After

```typescript
function skipped(a: number, b: number, enabled: boolean, chooseCaptured: boolean) {
  const captured = enabled && mystery(b);
  if (!enabled) return a + b;

  const fallback = b - a;
  return chooseCaptured ? captured : fallback;
}
```

### Expected behavior

Under !enabled, the call is skipped and return data flow is unchanged. Under enabled, the result/completion remains unknown even under !chooseCaptured: the call has already executed. The numerical checks separately define mystery(x)=x*3; the CLI receives no such model.

### Command

Working directory: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/05-skipped-call`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function skipped --format text --evidence-out full.json
```

### Command output — unchanged stdout

```text
Function result: skipped
Unchanged data flow under !enabled: (a + b) (runtime value equality unresolved)
Edit attribution uncertain
Unknown under enabled: normal completion or result selection is unresolved
Coverage: result comparison Partial; source alignment Partial; before analysis Partial; after analysis Partial
Evidence: full.json (ifds-function-2ed9604500d4d8b6)
```

### Verification and possible issues

Under !enabled, the call is indeed skipped and unchanged data flow a + b is shown. Under enabled, the command retains Unknown without claiming a proven change in the returned value. Even under !chooseCaptured, the unknown call has already executed.

- The Unknown reason is too general: normal completion or result selection is unresolved. The full evidence contains unsupported call_expression on line 2 and two observations with unknown_completion_before_return=true on each side. The user-facing text loses the reference to the unknown call.
- The result is correctly conservative, but the reason is insufficiently explained. The numerical model mystery(x)=x*3 in the concrete checks was not supplied to the CLI and does not justify replacing Unknown with Changed/Equal.

### Concrete checks

| Arguments in parameter order | Before | After |
|---|---|---|
| `[5, 2, false, false]` | `7` | `7` |
| `[5, 2, false, true]` | `7` | `7` |
| `[5, 2, true, false]` | `-3` | `-3` |
| `[5, 2, true, true]` | `15` | `6` |

<a id="6-06-nested-selectors"></a>

## 6. Two conditional sources, early exit, and sign reversal

Artifacts: [before.ts](06-nested-selectors/before.ts), [after.ts](06-nested-selectors/after.ts), [report.txt](06-nested-selectors/report.txt), [full.json](06-nested-selectors/full.json), [stderr.log](06-nested-selectors/stderr.log), [run.json](06-nested-selectors/run.json).

### Before

```typescript
function selectors(a: number, b: number, enabled: boolean, first: boolean, second: boolean, negate: boolean) {
  if (!enabled) return 0;

  const left = first ? a : b;
  const right = second ? b : a;
  const frozen = left - right;

  return negate ? -frozen : frozen;
}
```

### After

```typescript
function selectors(a: number, b: number, enabled: boolean, first: boolean, second: boolean, negate: boolean) {
  if (!enabled) return 0;

  const left = first ? a : b;
  const right = second ? a : b;
  const frozen = left - right;

  return negate ? -frozen : frozen;
}
```

### Expected behavior

Under enabled, the selected right operand changes in every first/second/negate combination. Under !enabled, the return value 0 is unchanged. Sources may coincide within a computation after the change; their operand roles must still be preserved.

### Command

Working directory: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/06-nested-selectors`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function selectors --format text --evidence-out full.json
```

### Command output — unchanged stdout

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

### Verification and possible issues

The eight changed enabled regions with different first/second/negate values are expanded correctly. The full JSON also establishes Equal for !enabled with return value 0.

- Confirmed omission in the short text: the !enabled branch is not shown at all. comparison.regions[0] contains Equal, value_dependency_changed=false, and condition {2:false}; this is a known unchanged branch, not Unknown.
- The unchanged-region grouping threshold requires at least two regions. A single Equal without control_changed then appears neither in the summary nor in the regular effect loop. Successful comparison alone therefore does not guarantee a complete short report.
- One source replacement in const right on line 5 produces eight Edit attribution uncertain messages and eight identical write-location lists. More precise conditional-selection attribution and a combined enabled change region are needed.
- Do not automatically simplify a - a to 0 for unrestricted inputs. Even ordinary numerical examples produce -0 for -(a - a); negative zero is preserved separately in runtime-checks.json.

### Concrete checks

| Arguments in parameter order | Before | After |
|---|---|---|
| `[5, 2, false, true, true, false]` | `0` | `0` |
| `[5, 2, true, true, false, false]` | `0` | `3` |
| `[5, 2, true, true, true, false]` | `3` | `0` |
| `[5, 2, true, false, false, false]` | `-3` | `0` |
| `[5, 2, true, false, true, false]` | `0` | `-3` |
| `[5, 2, true, true, true, true]` | `-3` | `-0` |

<a id="7-07-nullish-and-overwrite"></a>

## 7. Nullish selection, a saved copy, and an independent return

Artifacts: [before.ts](07-nullish-and-overwrite/before.ts), [after.ts](07-nullish-and-overwrite/after.ts), [report.txt](07-nullish-and-overwrite/report.txt), [full.json](07-nullish-and-overwrite/full.json), [stderr.log](07-nullish-and-overwrite/stderr.log), [run.json](07-nullish-and-overwrite/run.json).

### Before

```typescript
function nullish(a: number, b: number, useSaved: boolean) {
  let selected = a ?? b;
  const saved = selected + 1;

  selected = 100;
  if (useSaved) return saved;
  return selected;
}
```

### After

```typescript
function nullish(a: number, b: number, useSaved: boolean) {
  let selected = a ?? b;
  const saved = selected + 2;

  selected = 100;
  if (useSaved) return saved;
  return selected;
}
```

### Expected behavior

The saved computation changes under useSaved, while !useSaved returns 100. For unrestricted inputs, falsy does not mean nullish: 0 must keep a selected, while null must select b. A conservative Unknown for unresolved selection is a precision limit, not evidence of no change.

### Command

Working directory: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/07-nullish-and-overwrite`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function nullish --format text --evidence-out full.json
```

### Command output — unchanged stdout

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

### Verification and possible issues

For truthy a, the command correctly shows the saved change under useSaved. For falsy a, it retains Unknown: truthiness alone cannot distinguish 0 from null. Concrete checks confirm that 0 selects a while null selects b.

- Confirmed omission in the short text: the established Equal for a && !useSaved is missing. It exists in comparison.regions[1], and both sides return 100.
- Precision is lost under !a: Unknown also covers !useSaved, although the selected copy has already been overwritten with 100. Combining identical normal results from several nullish-selection alternatives deserves separate investigation; falsy must not simply be treated as nullish.
- The data-flow change for a=0 and useSaved is visible in direct execution (1 -> 2), but the CLI does not establish that branch. This is conservative incompleteness in the input-condition model, not a false equality claim.
- The !a condition in the report means falsy a, not a == null. The Unknown explanation does not describe this boundary, so users may misinterpret it.

### Concrete checks

| Arguments in parameter order | Before | After |
|---|---|---|
| `[5, 2, true]` | `6` | `7` |
| `[0, 2, true]` | `1` | `2` |
| `[null, 2, true]` | `3` | `4` |
| `[null, 2, false]` | `100` | `100` |
| `[0, 2, false]` | `100` | `100` |

<a id="8-08-reassociation"></a>

## 8. Arithmetic reassociation with rounding and early return

Artifacts: [before.ts](08-reassociation/before.ts), [after.ts](08-reassociation/after.ts), [report.txt](08-reassociation/report.txt), [full.json](08-reassociation/full.json), [stderr.log](08-reassociation/stderr.log), [run.json](08-reassociation/run.json).

### Before

```typescript
function association(a: number, b: number, c: number, skip: boolean, negate: boolean) {
  const total = (a + b) + c;
  const doubled = total * 2;

  if (skip) return 0;
  return negate ? -doubled : doubled;
}
```

### After

```typescript
function association(a: number, b: number, c: number, skip: boolean, negate: boolean) {
  const total = a + (b + c);
  const doubled = total * 2;

  if (skip) return 0;
  return negate ? -doubled : doubled;
}
```

### Expected behavior

The computation structure changes under !skip; under skip, the return value remains 0. Algebraic associativity does not prove equality for JS numbers: [1e16,-1e16,1,false,false] produces 2 -> 0. For [1,2,3,false,false], the values coincide even though data flow changed.

### Command

Working directory: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/08-reassociation`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function association --format text --evidence-out full.json
```

### Command output — unchanged stdout

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

### Verification and possible issues

Both !skip branches are correctly marked Changed despite using the same input set. The [1e16,-1e16,1,false,false] scenario indeed gives 2 -> 0. For [1,2,3,false,false], the values coincide, consistent with the structural meaning of Changed.

- Confirmed omission in the short text: skip with return value 0 is missing, although comparison.regions[2] establishes Equal and value_dependency_changed=false.
- Two repeated uncertain-attribution messages refer to one parenthesis edit on line 2. Tree-reassociation correspondence could be improved, with a combined !skip change region.
- A numerical witness does not prove inequality for all inputs. The report correctly retains Changed rather than a universal Different.

### Concrete checks

| Arguments in parameter order | Before | After |
|---|---|---|
| `[10000000000000000, -10000000000000000, 1, false, false]` | `2` | `0` |
| `[1, 2, 3, false, false]` | `12` | `12` |
| `[1, 2, 3, false, true]` | `-12` | `-12` |
| `[1, 2, 3, true, false]` | `0` | `0` |

<a id="9-09-loop-boundary"></a>

## 9. Loop accumulation and the zero-iteration branch

Artifacts: [before.ts](09-loop-boundary/before.ts), [after.ts](09-loop-boundary/after.ts), [report.txt](09-loop-boundary/report.txt), [full.json](09-loop-boundary/full.json), [stderr.log](09-loop-boundary/stderr.log), [run.json](09-loop-boundary/run.json).

### Before

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

### After

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

### Expected behavior

For positive finite integer count, the normal numerical result changes; count=0 returns 0. Loops are excluded from the initial function-result subset: the report must explicitly say Unknown/Unsupported rather than claim unchanged results or loop termination for all unrestricted inputs.

### Command

Working directory: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/09-loop-boundary`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function accumulate --format text --evidence-out full.json
```

### Command output — unchanged stdout

```text
Function result: accumulate
Edit attribution uncertain
Unknown under always: normal completion or result selection is unresolved
Coverage: result comparison Partial; source alignment Partial; before analysis Partial; after analysis Partial
Evidence: full.json (ifds-function-3a2566e347420eeb)
```

### Verification and possible issues

The command reports Unknown and Partial for both sides. There are no normal observations; the full JSON contains unsupported while_statement on lines 6–9. Loops are excluded from the current function-result subset.

- This is an expected support limitation, not a confirmed comparison error. Concrete finite executions show a difference for positive count but do not prove termination for arbitrary unrestricted inputs.
- The textual Unknown reason is again too general: it does not identify the unsupported while as the obstacle. The full evidence contains the precise reason, which should be included in the short report.

### Concrete checks

| Arguments in parameter order | Before | After |
|---|---|---|
| `[0, 3]` | `0` | `0` |
| `[1, 3]` | `4` | `5` |
| `[2, 3]` | `8` | `10` |
| `[3, 0]` | `3` | `6` |

<a id="10-10-unused-nested-function"></a>

## 10. Change to an uncalled nested function that captures inputs

Artifacts: [before.ts](10-unused-nested-function/before.ts), [after.ts](10-unused-nested-function/after.ts), [report.txt](10-unused-nested-function/report.txt), [full.json](10-unused-nested-function/full.json), [stderr.log](10-unused-nested-function/stderr.log), [run.json](10-unused-nested-function/run.json).

### Before

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

### After

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

### Expected behavior

helper is never called; its returns and changed computation must not enter the result of outer. Data flow of outer is unchanged for every input with an established normal return; global source alignment may remain Partial because the file changed.

### Command

Working directory: `/home/dima/Develop/repo-memory/spec/function-result-review/complex-batch-2026-10-03/10-unused-nested-function`. Exit code: **0**.

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml --example ifds_compare -- before.ts after.ts --function outer --format text --evidence-out full.json
```

### Command output — unchanged stdout

```text
Function result: outer
Unchanged data flow under always (runtime value equality unresolved)
Coverage: result comparison Partial; source alignment Partial
Evidence: full.json (ifds-function-116ab92915820665)
```

### Verification and possible issues

The changed helper body does not affect outer observations: helper is never called. The command correctly reports unchanged data flow over the entire common domain while retaining uncertainty about runtime equality of arithmetic.

- No clear error was found in the checked regions. Partial source alignment is acceptable: the file changed, but the change does not reach the return of outer.
- The report could explain that the diff is in an uncalled nested function. The current general unchanged-flow line does not explain why this particular edit is excluded from the result of outer.

### Concrete checks

| Arguments in parameter order | Before | After |
|---|---|---|
| `[5, 2, false]` | `-3` | `-3` |
| `[5, 2, true]` | `3` | `3` |
| `[2, 5, false]` | `3` | `3` |
| `[2, 5, true]` | `-3` | `-3` |

## Artifact verification and rerun

Checked 47 truthiness combinations against the full structured regions. This checks coverage and expected change indicators over the supported Boolean partition; it does not eliminate uncertainty about nullish selection, calls, loops, or runtime equality.

Fix priorities: first restore single Equal branches in the short text; then explain specific Unknown reasons; afterward improve attribution of swaps and conditional sources while preserving operand roles and write versions. Loop support and a more precise nullish model are separate analysis changes.

Commands are recorded in [runs.json](runs.json); sources and expectations in [cases.json](cases.json); concrete execution results in [runtime-checks.json](runtime-checks.json); verification summary in [verification.json](verification.json); analyzer state and source hashes in [engine-state.json](engine-state.json).

```sh
python3 spec/function-result-review/complex-batch-2026-10-03/run.py
python3 spec/function-result-review/complex-batch-2026-10-03/build_report.py
```

To check only the concrete JS scenarios without repeating the comparison:

```sh
python3 spec/function-result-review/complex-batch-2026-10-03/run.py --verify-only
```

The CLI protects existing evidence: if a future analyzer produces a different full.json, rerunning the comparison will fail. For a new review, use a fresh directory and preserve this result snapshot. run.py saves stdout/stderr from each new run; copy the entire archive before rerunning it.
