# K024 — Basic await and promise results

Status: planned. Roadmap stage: 6.

Dependencies: [K022](022-boundary-summaries.md), [K023](023-exceptions.md).

Source: [roadmap stage 6](../../docs/ifds/README.md) and
[SPEC uncertainty/lifecycle rules](../../docs/ifds/SPEC.md).

## Scope

- Model explicit await suspension plus fulfilled/rejected continuations and known
  promise result projections using resolved bodies or audited summaries.
- Preserve supported immutable local origins across suspension; expose external
  mutation/capture/scheduling uncertainty without inventing event order.
- Follow downstream supported consumers after resumption; a suspension is neither
  execution exit nor proof that the variable's influence ends.

Excludes arbitrary promise chains/combinators and framework scheduling, owned by K034/K036.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `fulfilled_continuation` | A known awaited result feeds only its fulfilled continuation and later supported consumers. |
| `rejected_continuation` | Rejection follows the correct catch/finally path, not the normal result assignment. |
| `local_survives_suspend` | An unaffected scalar copy retains its origin across await. |
| `mutable_capture_unknown` | A potentially externally modified capture has an explicit uncertainty frontier, not stale supported facts. |
| `request_not_response` | Known request-input origins do not become response origins without a model establishing that dependency. |
| `await_not_lifecycle_end` | Downstream uses after resumption remain included; unresolved continuation leaves an open ending. |
| `changed_result_source` | A changed known awaited producer updates surviving consumer provenance with fulfilled/rejected conditions preserved. |

## Verification

Run `cargo test --locked --offline --lib ifds_k024_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
