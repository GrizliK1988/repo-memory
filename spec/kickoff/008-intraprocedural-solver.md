# K008 — Single-procedure fixed-point solver

Status: done. Roadmap stage: 1.

Dependencies: [K003](003-test-harness.md), [K006](006-straight-line-ir.md),
[K007](007-transfer-functions.md).

Source: [SPEC section 4, procedures and solver](../../docs/ifds/SPEC.md).

## Scope

- Implement a worklist propagating only newly discovered finite facts; use union
  at joins and deduplicate scheduled work/evidence.
- Preserve enough predecessor information for typed flow witnesses. Support cyclic
  hand-authored IR now; TypeScript loop lowering belongs to K016.
- Expose counters/cancellation hooks for K009 without embedding wall-clock sleeps.

Excludes interprocedural summaries and asserting branch feasibility from reachability alone.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `straight_line_result` | Lowered overwrite/copy fixture matches reviewed facts at every read. |
| `join_unions_writers` | Hand-authored diamond preserves both reaching writers; the join creates no extra writer. |
| `cycle_converges` | A finite cyclic IR reaches the reference fixed point without unrolling or growing static IDs. |
| `unreachable_no_generation` | A disconnected write receives no Zero and produces no facts. |
| `worklist_order_independent` | Different queue/edge orders yield identical canonical facts and supported relations. |
| `reference_equivalence` | Enumerated small supported graphs agree with K003's independent evaluator. |

## Verification

Run `cargo test --locked --offline --lib ifds_k008_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
