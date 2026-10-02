# 005 — Make binding human reports more compact

Status: implemented and verified; separate from function-result report task 004.
See the [implementation review](005-implementation.md),
[verification results](005-verification.json), and
[fresh binding reports](binding-reports/README.md).

## Goal and scope

Apply concise human reporting to variable-flow queries targeting a binding.
The user explicitly requested that binding reports be handled separately from
[004](004-evidence-size.md), which changes only function-result text.

Fresh binding reports were inspected before agreeing the presentation below.
They repeat shared source changes across observations and shared use guards
across continuation points. Distinguish essential conclusions, scope, and
uncertainty from evidence details. Do not assume function return-choice tables
or result-equality claims apply to binding flow reports.

Use 004's presentation direction: concise human text by default, detailed text
on request, and full evidence by reference. Preserve JSON contracts, analysis/
comparison semantics, snapshot identity, report identity, and reference validation.
The binding identity implementation clears `human_summary` before hashing the
full report. Changing that field would still change full JSON content, so preserve
it as agreed below; do not change IDs or evidence content for the new text mode.
The [004 verification](004-implementation.md#measurements-and-parity) confirmed that
binding `stats.elapsed_ms` already affects IDs across independent CLI executions;
account for this existing limitation when measuring parity.

## Agreed presentation: shared changes before affected uses

The user selected a change-centered presentation: describe a shared source or
logic change once, then show its affected flow with source positions. Do not
repeat the same before/after explanation separately for each affected use.

For example, changing `x = input` to `x = input + 1` while retaining
`const y = x + 1; return y * 2;` can be presented as this finding fragment:

```text
Source changed:
  Before: x = input     (snippet.ts:2)
  After:  x = input + 1 (snippet.ts:2)

Affected flow (before/after):
  operand x in y = x + 1 (snippet.ts:3)
  -> operand y in return y * 2 (snippet.ts:4)
```

The fragment illustrates layout, not a complete report. Scope, coverage,
uncertainty, and the full-evidence reference remain required. Explicit snapshot
labels apply to the positions; differing before/after positions must remain
distinguishable. Each affected use retains its own condition when conditions
differ, and unresolved relationships retain their uncertainty. Sharing an
explanation does not change structured observation grouping or comparison facts.
The example establishes a changed dependency, not equality or inequality of the
whole function result.

### Transparent copies

Short text may omit an intermediate transparent copy, such as `const saved = x`,
when its origin is unambiguously established and remains clear in the displayed
flow. Keep computations such as `saved + 1` and `y * 2` visible. Do not omit a
step when doing so would hide relevant conditions, a binding/value version
distinction, multiple possible origins, or uncertainty. Copy elision is a
presentation rule only: detailed text and full evidence retain the complete chain.

### Explicit source positions

Short text includes source positions for changed sources, material controlling
conditions, and affected uses. Label each position with its before/after snapshot;
an unchanged position may be shared with an explicit `before/after` label. A
writer and its condition at the same position may share that position within the
finding. Do not repeat a position unnecessarily within a block, or hide the
affected-use position solely in the evidence reference. Positions of omitted
transparent copies remain available in detailed text and full evidence.

For a conditional write that changes both expression and guard, with the return
moving from line 4 to line 6, the finding fragment can be:

```text
Source and selection changed:
  Before: x = input + 1 under flag  (snippet.ts:3)
  After:  x = input + 2 under !flag (snippet.ts:4)

Affected use: operand x in return x * 2
  Before: snippet.ts:4
  After:  snippet.ts:6
```

### Selection changes with necessary context

Short text shows the changed selection rule once, with only the context needed
to interpret it: fallback sources, applicable use conditions, and material
overwrite precedence. It need not enumerate every source's complete before/after
selection rules; detailed text retains that detail. Preserve complete conditions
for each displayed claim and do not confuse an assignment guard with a guarantee
that its writer supplies the observed value when a later write can override it.

For a guard-only change, the finding fragment can be:

```text
Selection changed:
  Before: x = 2 under flag  (snippet.ts:3)
  After:  x = 2 under !flag (snippet.ts:3)
  Fallback: x = 1 (before/after snippet.ts:2)

Affected use: return x (before/after snippet.ts:4)
```

### Hide standalone write edits without established use impact

Short text omits standalone added, removed, or edited writes when no established
impact on an affected use is shown. For example, adding `x = 2` after
`const saved = x` does not warrant a separate short finding when the later
`return saved` continues to read the earlier copied value. Such write edits
remain in detailed text, structured reports, and full evidence.

This suppression does not hide analysis/comparison uncertainty or unresolved
findings, and does not establish the absence of impact outside the declared
scope. If no visible use-impact finding remains, describe only the absence of
established changes at uses within the scope; do not claim that no writes changed
or that results are equal. Intentional omission of standalone write details is
distinct from budget truncation of affected-use groups, whose omission counts
remain required.

### Detailed text compatibility and essential notices

`--verbose` retains the current binding presentation's level of detail, including
complete recorded dependency chains and standalone write edits. Exact byte-for-byte
compatibility with the previous human text is not required: detailed text may add
essential notices about query scope, entry assumptions, analysis/comparison
uncertainty, limits, presentation omissions, and open boundaries. Both short and
detailed modes retain these notices when applicable and link to the same full
evidence. Adding text detail must not change analysis/comparison semantics or
claim a complete lifecycle beyond the analyzed scope.

## Structured output and legacy summary compatibility

Preserve the existing `human_summary` in full JSON, including its current wording
and evidence-path handling. Keep the legacy summary renderer separate from the
new short/detailed presentation; neither text-mode choice nor new detailed-mode
notices may replace the stored summary. This applies to both API report assembly
and CLI sidecar writes, including the existing custom evidence-path behavior.

Full and compact JSON structures, field values, schema versions, structured
findings/grouping, and evidence references remain unchanged for identical fixed
report facts and evidence paths. Text modes consume existing structured evidence;
they do not add analysis, evaluate results, or infer edit attribution. Any missing
relationship or source position must remain explicitly unresolved rather than
be invented to fit the layout.

## CLI behavior

- Binding `--format text` defaults to the agreed short presentation.
- Extend the existing `--verbose` option to binding text targets. Function text
  behavior from 004 remains unchanged.
- Reject `--verbose` with either JSON format for either target, before source I/O
  or evidence writes.
- Both human modes link to the exact same full report for a fixed analysis and
  evidence path. Selecting verbosity must not change sidecar content or identity.
- Keep the default format, sidecar path selection, and existing overwrite checks.

## Verification cases

Eight fresh binding examples were generated during clarification using the
current `ifds_compare` example, binding `x`, containing-function scope, and default
analysis limits. Current text line counts include the scope/evidence footer.
They are synthetic readability examples, not a project performance estimate.

| Case | Current lines | Required short behavior |
| --- | ---: | --- |
| `full_downstream_chain` | 16 | Describe the shared source change once, retaining computations, operand roles, and downstream uses. |
| `early_return` | 11 | Keep the added return distinct; share continuation conditions without hiding affected positions or their guards. |
| `guard` | 7 | Describe guard polarity change once, with necessary fallback context and the affected use. |
| `overwrite` | 5 | Show the before/after reaching writers and use, distinguishing a retained but overwritten writer from a deleted write. |
| `unknown` | 4 | Keep unresolved comparison, unknown reasons, and partial coverage visible. |
| `priority` | 3 | Preserve the changed overwrite priority, including before/after meaning; shortening is not required at the expense of clarity. |
| `copy_then_overwrite` | 2 | Hide the standalone added write when no established use impact is shown; preserve the old origin of the saved copy in evidence. |
| `unchanged` | 2 | Keep a scoped absence-of-established-change statement without claiming result equality. |

The three named fixture cases are under `tests/ifds/fixtures`; the other five use
the existing guard-only, reordered-priority, early-return, unsupported-call, and
unchanged patterns from binding analysis tests. Save exact source pairs and query
provenance with fresh implementation verification so measurements can be reproduced.
Also check transparent copies versus computations, differing snapshot positions,
nested conditions and distinct guard value versions, non-equivalent observations,
entry/boundary restrictions, and analysis versus presentation truncation.

## Acceptance criteria

- Record agreed before/after binding examples and demonstrate less repetition
  without hiding material flow changes or uncertainty.
- Show evidence-backed relationships and source positions, preserving snapshot
  distinctions and complete retrievable evidence.
- Verify deterministic text for a fixed structured report. All short claims must
  agree with detailed text and full evidence; the documented detail elisions do
  not require identical claim lists in both modes.
- Verify structured-output parity, including unchanged legacy `human_summary`,
  and valid references to the exact full report. For independent CLI runs,
  normalize only documented runtime stats, derived IDs, and temporary evidence
  paths; do not normalize away changed summary wording or other evidence content.
- Compare before/after lines and bytes on exact source pairs and queries. There
  is no fixed line/byte cap; explicit positions or essential notices may increase
  individual reports even while shared explanations become less repetitive.
- Keep function-result behavior from 004 unchanged.
- Document CLI behavior and run focused binding renderer, evidence-validation,
  and CLI checks. Preserve archives and save fresh verification separately.
