# 004 — Make the human report more compact

Status: implemented and verified. See the [implementation review](004-implementation.md)
and [measurements and parity checks](004-verification.json).

## Goal and agreed scope

Make the human-readable function-result report shorter and easier to scan.
This follows [003](003-compact-readable-text.md), whose implemented renderer
still repeats simple replacements in choice tables and effect lines and prints
detailed contributing-write lists.

The user clarified that the intended improvement concerns the human report.
The earlier proposal to deduplicate JSON metadata and dependency trees is
superseded by this scope.

Change text rendering, its CLI presentation options, and documentation. Preserve
full and compact JSON structures, schema versions, report IDs, references, and
analysis/comparison semantics. Complete evidence stays in the full sidecar;
the short text need not contain every structured fact.

Binding reports are a separate task:
[005 — Make binding human reports more compact](005-binding-human-report.md).

## Agreed presentation

### Short output by default; detailed output on request

Make concise text the default for function targets with `--format text`.
Add `--verbose` for detailed human-readable output using the current 003
presentation as its baseline, including choice tables and detailed source links.
Both modes derive their claims from the same existing facts and reference the
same full report.

Keep the default format as text. In this task, `--verbose` applies only to
function text output; reject unsupported combinations explicitly rather than
silently changing JSON output or binding rendering. Document this boundary.

### Choice tables only when they explain conditional selection

For simple literal, copy, or expression replacements, show each before/after
effect once without a table repeating the transition as `always`/`never` rows.
Preserve proven-value annotations and the established assessment.

Retain a table for complex conditional selection when it explains before/after
selection rules beyond the effect statements. A single-region replacement under
a condition needs only the condition and effect. A grouped conditional swap such
as `guard_version` retains its single table and both directional effects.

Preserve full input conditions and their relationship to common contexts. Do not
weaken truthiness to Boolean equality or regroup comparison findings to shorten
presentation. Preserve the selected-operand wording established in 002.

### Key contributing writes with source positions

In short text, show key contributing assignments with before/after source
positions instead of repeating every return location and dependency link.
Use existing dependency evidence to identify the writes supplying the displayed
value; omit intermediate transparent copies when the value's origin stays clear.
Do not identify overwritten writes as contributors or present a contributing
write as a confidently attributed edit when attribution is uncertain.

Keep multiple relevant contributors when a single source is not established;
do not choose one arbitrarily. Deduplicate source links within a finding without
merging before/after snapshot identity. Detailed text retains the current
source-link presentation; full evidence retains every dependency relationship.

### Explicit control changes, including equal results

Show established control changes explicitly, even when the returned result is
equal. Preserve 003's policy: show actual before/after conditions when their
correspondence is established and state correspondence uncertainty otherwise.
Retain the applicable input scope. For example, `equal_control` still conveys:

```text
Control: flag -> !flag; result remains 1 (Equal)
```

Do not repeat that equality in an ordinary equal-region list, table, or unchanged
summary. Ordinary equal regions stay hidden. Retain the scoped unchanged-result
summary only under the complete-equality conditions agreed in 003.

When one control finding has multiple proven equal result choices, short text
may summarize equality over the union of their regions without listing each
unchanged value. Detailed text retains those individual values and regions.
This scoped summary must not imply equality in the changed or unknown regions.

### Essential uncertainty, scope, and evidence remain visible

Retain unknown regions and reasons, function presence/counterpart status, and
entry/common-input restrictions needed to interpret the conclusions. An empty
visible finding list does not establish complete equality.

Keep 003's concise coverage line: independent result-comparison and source-
alignment status, plus other incomplete dimensions, limits, and presentation
omissions when applicable. Keep one uncertain-edit-attribution marker per visible
finding and a link to the exact full evidence report in both human modes.

## Acceptance criteria

| Case | Required default text behavior |
| --- | --- |
| `literal`, `copy`, `known_expression` | One effect per distinct transition; no redundant table. Preserve expression/value meaning, assessment, and key contributing-write positions. |
| `fallthrough`, `short_circuit` | Show the condition and replacement once without a redundant table; preserve implicit `undefined`, operand selection, and truthiness. |
| `guard_version` | One conditional table and both directional effects; no repeated source links or attribution markers per effect. |
| `guard`, `equal_control` | Explicit established control changes, including equal-result changes and unresolved guard correspondence; preserve the claim's input scope. |
| `priority`, dependency chains | Preserve selection rules and relevant value origins; no overwritten assignment shown as an origin. |
| `expression`, `unknown`, executed unsupported calls | Visible unknown reasons and incomplete coverage alongside independent known effects; preserve conservative assessments. |
| Fully equal results, scoped queries, missing counterparts, disjoint domains, truncated output | Preserve 003's notices and safeguards. |

- `--verbose` exposes the existing detailed function presentation, including
  simple choice tables and source links, without changing the query or evidence.
- JSON remains identical for identical queries, apart from sidecar-path
  normalization between verification runs. Binding output remains unchanged.
- Short and detailed text are deterministic and semantically consistent.
  Every displayed source position and evidence reference resolves to the correct
  snapshot and full report. Archived reports remain intact.

Judge compactness by the agreed examples and removal of repetition, without a
fixed line/byte limit that would hide necessary findings. Record before/after
line counts and byte sizes for freshly generated human reports of the same source
pairs and queries, distinguishing short and detailed modes. These synthetic
examples are not a project performance estimate.

Run focused renderer and CLI checks for both text modes and invalid option
combinations, relevant function-result unit/integration checks, and repository
formatting/lint checks. Exercise all three CLI formats with fresh sidecars,
verify structured-output parity, and save verification artifacts separately
from archived reports before marking this task complete.
