# K035 — Generators and iteration protocols

Status: planned. Roadmap stage: 10.

Dependencies: [K016](016-loops.md), [K024](024-basic-await.md),
[K030](030-closures.md).

Source: [roadmap stage 10](../../docs/ifds/README.md).

## Scope

- Model generator yield/resume/return, supported iterator protocols, `for...of`,
  and `for await...of` with explicit suspension and iteration-result projections.
- Track supported values passed into resumption and yielded to consumers; handle
  iterator closing and cleanup on abrupt completion where resolved/modeled.
- Retain finite cyclic evidence and explicit boundaries for custom unknown iterators.

Excludes equating generator construction with running its body or exhausting every iterator.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `construction_is_lazy` | Creating a generator does not execute its first assignment/yield before a modeled next call. |
| `yield_resume_values` | Yielded origins reach the correct consumer and next's input reaches the matching resume expression. |
| `for_of_projection` | Supported array/iterator elements feed iteration bindings without mixing unrelated element fields. |
| `async_iteration_paths` | Async iterator fulfillment/rejection uses the corresponding loop/exception paths. |
| `abrupt_iterator_close` | Supported break/throw paths perform required iterator/finally cleanup before leaving. |
| `unknown_iterator` | Unknown protocol methods create effect/schedule boundaries, not fabricated element sources. |
| `finite_iteration_evidence` | Static IDs and compact cycles remain bounded; a live suspended generator is not labeled a completed lifecycle. |

## Verification

Run `cargo test --locked --offline --lib ifds_k035_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
