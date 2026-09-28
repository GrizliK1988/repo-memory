# 003 — Make the short report more concise and precise

Status: presentation suggestions based on the examples.

## Repeated lines

For `equal_control`, the report prints a `Return choice` table, `Effect: always: 1 -> 1 (Equal)`, `Unchanged choices: 1`, and `Equal under always: 1`. These repeat the same result four times, although the main point is the changed guard with an unchanged returned value. One line about the changed control and equal result should be enough; avoid repeating the same choice in both findings and the unchanged summary.

For `guard_version`, two tables show the same `"yes"` and `"no"` choices under reversed conditions, followed by two effect lines. A single table with both input regions and one note that the result assignments were swapped would be clearer. This grouping is now owned by the agreed requirements in [001](001-report-values-and-attribution.md#symmetric-guard-effects); 003 retains the other readability work.

## Coverage and attribution visibility

In all these examples, analysis and result comparison are complete, but `source_alignment` is `Partial`. The text does not show this, while the full JSON does. The compact report also often has `attribution_certain: false` for `Different` findings. The short text links to the evidence file but does not indicate that the specific edit has not been confidently linked to the return site.

Show a concise source-alignment status and indicate uncertain attribution alongside a finding. Keep this separate from result comparison completeness: `source_alignment` and `result_comparison` measure different things.

## Evidence size

For these tiny examples, text reports are 160–456 bytes, compact JSON is about 4–12 KB, and full JSON is about 15–261 KB. The full dependency tree is useful for audits and references; the compact projection is enough for an overview, with full evidence in a sidecar. If size becomes a concern, consider representing shared metadata and nodes without losing completeness or reproducibility. These are synthetic example measurements, not a project performance estimate.

## Limits

`expression` without a numeric input domain correctly remains `unknown`: the analyzer does not infer the input type from this fixture. `known_expression` explicitly sets `input = 4`; the analyzer reports `input + 1 → input + 2` as `Changed` without evaluating it to `5 → 6`. This matches the structural expression comparison contract. Do not treat this conservative result as a reporting defect.
