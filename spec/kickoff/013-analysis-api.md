# K013 — Public API, reports, and first milestone

Status: planned. Roadmap stage: 1.

Dependencies: [K003](003-test-harness.md), [K004](004-benchmark-scoring.md),
[K009](009-uncertainty-and-limits.md), [K012](012-flow-comparison.md).

Source: [SPEC sections 1, 3, 6.4 and 7](../../docs/ifds/SPEC.md).

## Scope

- Wire `analyze_variable_flow` from validated immutable snapshots through parsing,
  solving, slicing, alignment, and comparison for the first supported subset.
- Build reproducible reports with full context, precise groups, source locations,
  witnesses, assumptions, model versions, endings, directional coverage, and a
  concise human summary derived from the structured evidence.
- Preserve consumer metadata for later historical alignment without claiming a
  persistent cross-history identity or implementing history mining.

Excludes branches/calls as supported semantics; relevant occurrences stay explicit boundaries.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `straight_line_api` | In-memory before/after repositories exercise the public API and exactly match case 1/2 required and forbidden claims. |
| `full_downstream_api` | Case 10 reports unchanged B/D/R, their source deltas, and the escaping-return boundary. |
| `report_round_trip` | Real assembled reports preserve all facts/evidence and metadata after serialization and reload. |
| `deterministic_groups` | Reordering independent processing produces the same canonical deltas and group IDs. |
| `partial_is_visible` | Unsupported input returns explicit per-region/direction uncertainty; it is not rendered as “no changes” or “no further impact.” |
| `history_metadata_only` | Consumer path, owner, input projection, span, fingerprint and pair alignment are present; no unproved bug-cause assertion appears. |
| `precise_change_record` | Case 2's structured group names added B, retained writer A, replaced source at input R, and any applicable boundary; a generic function-edit record is insufficient. |
| `human_summary_contract` | Render case 2 and a partial-boundary case: name the changed operation, whether its selected-binding write was added/removed/retained, affected consumer inputs, before/after sources or conditions, and every applicable unknown boundary; omit inapplicable fields and forbid generic “function/data flow changed,” “no further impact” at a boundary, or unsupported causal wording. |

## Verification

Run `cargo test --locked --offline --lib ifds_k013_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
Promote stage-1 exact fixtures through `cargo test --locked --offline --test ifds`;
all K001–K013 tests must be implemented, discovered, and passing before stage 1 closes.
