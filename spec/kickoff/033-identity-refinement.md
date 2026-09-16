# K033 — Alignment and dispatch refinement

Status: planned. Roadmap stage: 9.

Dependencies: [K032](032-path-feasibility.md), [K011](011-node-alignment.md),
[K017](017-project-metadata.md).

Source: [roadmap stage 9](../../docs/ifds/README.md) and
[SPEC identity rules](../../docs/ifds/SPEC.md).

## Scope

- Refine ambiguous operation matching using additional structural/metadata
  evidence and resolved dispatch candidates without overriding explicit selectors.
- Preserve unresolved candidate sets when evidence still does not identify a
  unique counterpart/target; continue comparing independent unambiguous regions.
- If exposing inferred variable candidates as a convenience, require a unique
  justified choice or explicit selection. Never silently analyze multiple bindings.

Excludes using line similarity, types, or candidate rankings as proof by themselves.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `extra_evidence_disambiguates` | A previously ambiguous match with new unique declaration/structural evidence aligns to that counterpart only. |
| `ambiguity_remains` | Indistinguishable repeated operations stay ambiguous and produce no definitive node removal/addition for affected candidates. |
| `explicit_selection_wins` | A valid explicit counterpart is preserved; stale contradictory input is rejected rather than silently changed. |
| `dispatch_refinement` | Proven runtime-compatible receiver evidence narrows targets; a type-only narrowing cannot fabricate a unique target. |
| `stable_unaffected_deltas` | Refining one region does not change unrelated logical identities or proven deltas. |
| `one_binding_contract` | Ambiguous inferred candidates cannot create a multi-binding query; the core still requires exactly one valid selected declaration. |
| `reverse_alignment` | Reversing uniquely aligned snapshots reverses additions/removals without losing ambiguity diagnostics elsewhere. |

## Verification

Run `cargo test --locked --offline --lib ifds_k033_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
