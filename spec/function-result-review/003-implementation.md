# 003 implementation review

[003](003-compact-readable-text.md) is implemented for the agreed text-only scope.
The renderer shows one scoped equal-result control-change line, a scoped summary
for completely equal results without a control change, coverage on every report,
and one uncertain-edit-attribution marker per visible finding. Changed-result
choice tables and the selected-operand presentation from 002 are preserved.

Concrete guard correspondence is conservative: unique paired-input dependencies
and stable contributing return structure must establish the inversion. An
unambiguous recorded source condition is shown directly. When only a resolved
condition is available, it is explicitly labeled `resolved`; unresolved guard
correspondence remains visible without an invented before/after pair. Tests cover
nested scopes, renamed parameters, reordered independent guards, mixed equal and
changed regions, partial comparison, truncated presentation, missing counterparts,
and disjoint input domains.

## Structured-output verification

All 13 archived source pairs were rerun in text, compact JSON, and full JSON modes
using fresh evidence sidecars. Full JSON stdout is byte-identical to the pre-003
baseline at commit `215bb01`. Compact JSON is identical after normalizing only
`evidence_file` to account for different temporary directories. Full JSON stdout
and its sidecar match apart from the final newline. Full report schema 2, analysis
schema 4, compact schema 2, report IDs, assessments, proofs, coverage facts,
attribution, and references are preserved. Archived reports remain unchanged.

[003-verification.json](003-verification.json) records report IDs, hashes, coverage,
and parity results for every case. The full hash covers stdout including its final
newline; the compact hash covers JSON with sorted keys, compact separators, and
`evidence_file` normalized to `full.json`.

## Regenerated text

These excerpts were generated from the original [review sources](EXAMPLES.md).
`full.json` in each excerpt denotes its freshly generated temporary sidecar,
not the archived sidecar. The CLI's logical `snippet.ts` path refers to each
case's before/after source according to the printed snapshot side.

### copy

Sources: [before](reports/copy/before.ts) and [after](reports/copy/after.ts).

```text
Function result: result
Edit attribution uncertain
Return choice | Before | After
saved (1) | always | never
saved (2) | never | always
Effect: always: saved (1) -> saved (2) (Different)
Before return saved at snippet.ts:5; contributing writes: snippet.ts:3, snippet.ts:2
After return saved at snippet.ts:5; contributing writes: snippet.ts:3, snippet.ts:2
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-c1143f1c678a1ce3)
```

### early_return

Sources: [before](reports/early_return/before.ts) and [after](reports/early_return/after.ts).

```text
Function result: result
Control changed (guard correspondence unresolved); result remains "normal" (Equal) under !stop in the common input scope; Edit attribution uncertain
Edit attribution uncertain
Return choice | Before | After
"early" | never | stop
"normal" | always | !stop
Effect: stop: "normal" -> "early" (Different)
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-dd502e03f5f90a07)
```

### equal_control

Sources: [before](reports/equal_control/before.ts) and [after](reports/equal_control/after.ts).

```text
Function result: result
Control: flag -> !flag; result remains 1 (Equal) under always in the common input scope; Edit attribution uncertain
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-c1627061de216565)
```

### expression

Sources: [before](reports/expression/before.ts) and [after](reports/expression/after.ts).

```text
Function result: result
Unknown under always: result value theory is insufficient
Coverage: result comparison Partial; source alignment Partial
Evidence: full.json (ifds-function-49f5f1bbdb6dd94b)
```

### fallthrough

Sources: [before](reports/fallthrough/before.ts) and [after](reports/fallthrough/after.ts).

```text
Function result: result
Context: !flag
Edit attribution uncertain
Return choice | Before | After
2 | never | always
undefined | always | never
Effect: always: undefined -> 2 (Different)
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-3ec23b63948b2cba)
```

### guard

Sources: [before](reports/guard/before.ts) and [after](reports/guard/after.ts).

```text
Function result: result
Control changed (guard correspondence unresolved); result remains "ok" (Equal) under enabled && !blocked && ready && approved in the common input scope; Edit attribution uncertain
Control changed (guard correspondence unresolved); result remains "pending" (Equal) under enabled && !blocked && !ready in the common input scope; Edit attribution uncertain
Context: enabled && !blocked
Edit attribution uncertain
Return choice | Before | After
"ok" | ready | ready && approved
"pending" | !ready | !ready || !approved
Effect: ready && !approved: "ok" -> "pending" (Different)
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-516461b3d9ed51a8)
```

### guard_version

Sources: [before](reports/guard_version/before.ts) and [after](reports/guard_version/after.ts).

```text
Function result: result
Edit attribution uncertain
Return choice | Before | After
"no" | !flag | flag
"yes" | flag | !flag
Effect: !flag: "no" -> "yes" (Different)
Effect: flag: "yes" -> "no" (Different)
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-a598f5826cd9b5c6)
```

### known_expression

Sources: [before](reports/known_expression/before.ts) and [after](reports/known_expression/after.ts).

```text
Function result: result
Return choice | Before | After
input + 1 | always | never
input + 2 | never | always
Effect: always: input + 1 -> input + 2 (Changed)
Before return input + 1 at snippet.ts:3; contributing writes: snippet.ts:2
After return input + 2 at snippet.ts:3; contributing writes: snippet.ts:2
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-2c8828ab71e2dbbd)
```

### literal

Sources: [before](reports/literal/before.ts) and [after](reports/literal/after.ts).

```text
Function result: result
Return choice | Before | After
1 | always | never
2 | never | always
Effect: always: 1 -> 2 (Different)
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-f0f31433ea1131ad)
```

### overwritten

Sources: [before](reports/overwritten/before.ts) and [after](reports/overwritten/after.ts).

```text
Function result: result
Normal results unchanged on common inputs: always (under the recorded entry assumptions).
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-f4dc3cfc1e5fa881)
```

### priority

Sources: [before](reports/priority/before.ts) and [after](reports/priority/after.ts).

```text
Function result: result
Control changed (guard correspondence unresolved); result remains x (0) (Equal) under !a && !b in the common input scope; result remains x (1) (Equal) under a && !b in the common input scope; result remains x (2) (Equal) under !a && b in the common input scope; Edit attribution uncertain
Edit attribution uncertain
Return choice | Before | After
x (1) | a && !b | a
x (2) | b | !a && b
Effect: a && b: x (2) -> x (1) (Different)
Before return x at snippet.ts:5; contributing writes: snippet.ts:4
After return x at snippet.ts:5; contributing writes: snippet.ts:4
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-bb514a929c428eee)
```

### short_circuit

Sources: [before](reports/short_circuit/before.ts) and [after](reports/short_circuit/after.ts).

```text
Function result: result
when flag is truthy: "old" -> "new" (Different)
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-d805e525daaf76d7)
```

### unknown

Sources: [before](reports/unknown/before.ts) and [after](reports/unknown/after.ts).

```text
Function result: result
Context: flag
Return choice | Before | After
1 | always | never
2 | never | always
Effect: always: 1 -> 2 (Different)
Edit attribution uncertain
Unknown under !flag: an unresolved call may prevent normal completion
Coverage: result comparison Partial; source alignment Partial; before analysis Partial; after analysis Partial
Evidence: full.json (ifds-function-d379155b81a46d01)
```

## Reproduction and checks

From the repository root, choose a fresh sidecar path:

```sh
cargo run --locked --offline --example ifds_compare -- \
  spec/function-result-review/reports/equal_control/before.ts \
  spec/function-result-review/reports/equal_control/after.ts \
  --function result --format text --evidence-out /tmp/review-003-equal-control-full.json
```

Select another case's source pair and use `compact-json` or `full-json` for the
other formats. A sidecar with different content is rejected; preserve archived
sidecars when reproducing the reports.

Verification passed: `cargo test --locked --offline` (217 unit tests, 9 integration
tests, 2 documentation tests), `cargo fmt --all -- --check`, and
`cargo clippy --locked --offline --all-targets -- -D warnings`. Serialization
round trips and evidence validation are covered by the renderer and integration
regressions. Evidence size reduction remains the separate planned task
[004](004-evidence-size.md).
