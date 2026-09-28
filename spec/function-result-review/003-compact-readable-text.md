# 003 — Make the short report more concise and precise

Status: implemented and verified. See the [implementation review and regenerated
text](003-implementation.md) and [structured-output parity checks](003-verification.json).

## Agreed scope

Limit this task to human-readable text output and the documentation describing
that output. Preserve the full and compact JSON structures and their schema
versions, as well as result assessments, proofs, coverage facts, and evidence
references. Text continues to derive from the existing structured facts.

Evidence size reduction is a separate task:
[004 — Reduce evidence size without losing information](004-evidence-size.md).
The requirements below record the agreed and implemented presentation decisions.

## Agreed equal-result control-change exception

Continue to omit ordinary proven equal regions and unchanged choices from text,
as agreed in [002](002-short-circuit-precision.md#presentation-and-structured-evidence).
When the existing evidence establishes a control change with an equal returned
result, retain one concise line communicating both facts. For `equal_control`,
the line must convey that control changed while the result remains `1` (`Equal`).
Do not repeat that equal result in a choice table, effect line, or unchanged
summary. Preserve the equal regions and their evidence in compact and full JSON.

This is an exception to the text omission rule for equal results, not a change
to result comparison. The exception applies to control changes; equal findings
with only value-dependency or return-structure changes retain the existing text
omission policy.

## Agreed concrete condition display

Show the actual condition before and after the control change when the existing
evidence establishes their correspondence. A generic `Control changed` notice
alone is insufficient when those conditions are established. For `equal_control`,
the single line must communicate:

```text
Control: flag -> !flag; result remains 1 (Equal)
```

This example specifies the required meaning, not exact punctuation. Derive the
condition expressions from the corresponding snapshots' existing evidence;
matching source text or names alone does not prove correspondence. Retain the
finding's input scope when equality holds only in a restricted region, rather
than implying that all function results are equal.

When control change is established but correspondence between specific
conditions is unresolved, show only the supported control-change fact and make
that correspondence uncertainty explicit. Do not invent a before/after guard
pair. Keep evidence references available and preserve the existing JSON schemas.

## Agreed unchanged-result summary

When both functions are present, the common input domain is established and
nonempty, all its feasible comparison regions are proven equal, and presentation
is complete, show one `Normal results unchanged` line if there is no established
control change to report. Ordinary equal regions and unchanged choices remain
hidden. State the applicable common-input/query scope rather than extending
equality to one-sided input domains or unrestricted execution.

Determine this from the complete structured facts, not merely an empty list of
visible findings. Unknown regions, unresolved input feasibility or correspondence,
and analysis/comparison limits preventing a complete equality claim disallow
the summary. Partial comparison or presentation also disallows it. Partial source
alignment alone does not invalidate an otherwise proven result-equality claim.

When a control-change/equal-result line is shown, do not repeat its equality in
an unchanged-result summary. If comparison is unresolved or output is truncated,
show the corresponding uncertainty or omission status. If there are no common
inputs, retain the existing separate flows and `No common inputs` notice without
a result relation. Added, removed, or unresolved functions retain their existing
presence/flow notices.

## Agreed choice-table policy

Remove repeated equal-result presentation. Preserve the existing choice tables
and effect statements for changed results, including simple literal and copy
changes. For `copy`, retain the choice table and
`saved (1) -> saved (2) (Different)` with the existing contributing-write links.
Selection conditions, factored contexts, and effect regions keep their complete
meaning and existing relationships.

Preserve the operand-selection presentation agreed in 002: the short-circuit
example shows `when flag is truthy: "old" -> "new" (Different)` without restoring
a choice table or its hidden equal region. Symmetric guard grouping remains
owned by 001; do not regroup findings as part of 003.

## Background: repeated lines

The archived `equal_control` report prints a `Return choice` table, `Effect: always: 1 -> 1 (Equal)`, `Unchanged choices: 1`, and `Equal under always: 1`. These repeat the same result four times. After 002, the current renderer omits the equal finding entirely, hiding the changed control as well. The agreed exception above restores one line about changed control and equal result, without restoring those repetitions.

For `guard_version`, two tables show the same `"yes"` and `"no"` choices under reversed conditions, followed by two effect lines. A single table with both input regions and one note that the result assignments were swapped would be clearer. This grouping is now owned by the agreed requirements in [001](001-report-values-and-attribution.md#symmetric-guard-effects); 003 retains the other readability work.

## Coverage and attribution visibility

In all these examples, analysis and result comparison are complete, but `source_alignment` is `Partial`. The text does not show this, while the full JSON does. The compact report also often has `attribution_certain: false` for `Different` findings. The short text links to the evidence file but does not indicate that the specific edit has not been confidently linked to the return site.

### Agreed coverage display

Always show one concise coverage line, even when comparison is complete:

```text
Coverage: result comparison Complete; source alignment Partial
```

Always include result-comparison and source-alignment status independently.
Include non-complete analysis status for each available snapshot, input-mapping
status, and presentation status when applicable. For truncated presentation,
include the omitted-group count and retain the evidence link. Keep analysis,
comparison, and presentation limits distinct; source alignment must not be
rendered as result-comparison uncertainty. The example specifies meaning, not
exact punctuation or field order.

### Agreed attribution display

Show `Edit attribution uncertain` alongside each visible finding whose existing
`attribution_certain` flag is false, including the equal-result control-change
exception. Show the marker once per finding, including grouped findings with
multiple directional effects; do not repeat it on each effect arrow. A finding
with certain attribution needs no uncertainty marker.

This describes uncertainty in linking the finding to a specific source edit.
It does not weaken an established `Different`, `Changed`, or `Equal` result
assessment or an established value dependency. Preserve contributing-write
links and evidence references. Do not infer attribution from a proven value or
from partial source alignment, and do not change the attribution algorithm.

## Evidence size

For these tiny archived examples, text reports are 160–456 bytes, compact JSON is about 4–12 KB, and full JSON is about 15–261 KB. These are synthetic example measurements, not a project performance estimate or a baseline for the current schemas. The full dependency tree remains available in the evidence sidecar. Investigation and reduction of repeated metadata and nodes belong to [004](004-evidence-size.md), outside this task.

## Limits

`expression` without a numeric input domain correctly remains `unknown`: the analyzer does not infer the input type from this fixture. `known_expression` explicitly sets `input = 4`; the analyzer reports `input + 1 → input + 2` as `Changed` without evaluating it to `5 → 6`. This matches the structural expression comparison contract. Do not treat this conservative result as a reporting defect.

## Acceptance criteria

| Case | Required text behavior |
| --- | --- |
| `equal_control` | One line conveys `flag -> !flag` and result `1 (Equal)`; no repeated equal table, effect, unchanged-choice list, or unchanged-result summary. |
| Unresolved guard correspondence | Show the established control change and correspondence uncertainty without inventing a concrete guard pair. |
| Scoped equal control change | Preserve the applicable input region/context; do not claim equality for other regions. |
| Fully equal results without a control change | One scoped `Normal results unchanged` summary, with ordinary equal regions hidden. |
| Partial comparison or presentation | No unchanged-result summary, even if all visible regions are equal or no findings are visible; include uncertainty/omission status and omitted-group count for truncation. |
| No common inputs or missing counterpart | Retain the existing domain/presence notices and side flows; no unchanged-result summary or invented cross-version relation. |
| `copy`, `literal`, `priority` | Preserve existing changed-result choice tables, complete selection conditions, effects, proven-value annotations, and contributing-write links. |
| `guard_version` | Preserve the one finding/table and both directional effects established by 001. |
| `short_circuit` | Preserve the selected-value effect and truthiness wording; hide the equal region and choice table as agreed in 002. |
| `unknown`, skipped unsupported calls | Keep unknown regions and reasons visible alongside independent known effects; do not infer complete comparison. |
| Every text report | Include result-comparison and source-alignment status; include other incomplete coverage dimensions and presentation omissions when applicable. |
| Uncertain edit attribution | Exactly one marker per visible uncertain finding, including grouped and equal-control findings; preserve its proven result assessment. |
| `expression`, `known_expression` | Preserve `Unknown` / `Changed` and their reasons; no arithmetic evaluation or operand-as-result annotation. |
| Structured reports | JSON structures, versions, semantic facts, evidence links, and attribution remain unchanged; hidden equal evidence stays retrievable. |

## Implementation verification

Add focused renderer unit and integration checks for the agreed cases, including
negative unchanged-summary cases and attribution markers on grouped findings.
Verify text claims against existing compact/full facts and preserve deterministic
ordering and evidence-reference validation. Run the relevant function-result
tests and the repository formatting/lint checks.

Rerun the review source pairs through the CLI in text, compact JSON, and full JSON
formats with fresh evidence sidecars. Record the regenerated text separately from
the archived reports and confirm structured-output parity with the pre-003
baseline for identical queries. Align the presentation documentation, including
the 002 equal-region omission rule, with the control-change exception and scoped
unchanged-result summary when implementing. Preserve archived evidence.
