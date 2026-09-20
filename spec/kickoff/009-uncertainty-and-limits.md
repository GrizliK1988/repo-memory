# K009 — Unknown effects and deterministic limits

Status: done. Roadmap stage: 1.

Dependencies: [K002](002-snapshots-and-queries.md), [K008](008-intraprocedural-solver.md).

Source: [SPEC sections 6.2, 7–8](../../docs/ifds/SPEC.md).

## Scope

- Track supported/modeled/unresolved evidence, per-region and directional
  completeness, unknown frontiers, and present/absent/unknown relation states.
- Preserve only facts proven unaffected by unknown operations; never treat an
  unmodeled call as identity or certify `does_not_write` from missing syntax.
- Enforce time, path-edge, memory-accounting, and output budgets with injectable
  clocks/counters. Distinguish witness-only truncation from missing graph results.

Excludes implementing the unsupported operations themselves or measuring real RSS here.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `unknown_call_boundary` | An unresolved relevant effect leaves a known prefix plus diagnostic/frontier; uncertain post-call facts are not labeled supported. |
| `unaffected_region_survives` | A partial branch does not discard independently proved relations in another region. |
| `no_absence_from_empty` | An empty incomplete edge set is unknown, not absent or `does_not_write`. |
| `deterministic_budget` | Advance fake time or edge/memory counters across the configured limit; return partial results and the exact limit reason. |
| `witness_vs_graph_limit` | Limiting displayed witnesses retains solved facts with omitted counts; truncating graph/flow output is explicitly incomplete. |
| `diagnostic_location` | Parse, missing dependency, unsupported syntax, and call failures identify affected snapshot, node/span, and direction. |

## Verification

Run `cargo test --locked --offline --lib ifds_k009_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
