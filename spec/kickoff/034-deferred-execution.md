# K034 — Deferred promises, callbacks, and events

Status: planned. Roadmap stage: 10.

Dependencies: [K024](024-basic-await.md), [K030](030-closures.md).

Source: [roadmap stage 10](../../docs/ifds/README.md) and
[SPEC full-lifecycle requirements](../../docs/ifds/SPEC.md).

## Scope

- Add individually audited promise-chain/combinator, timer, and event-registration
  models with separate registration and execution nodes.
- Use a finite explicit scheduling abstraction; preserve possible invocation order,
  mutable capture effects, and values that outlive the registering function.
- Unsupported scheduling yields `UnknownSchedule`, not synchronous flattening
  or proof that a callback never runs. Publish model assumptions in every report.

Excludes executing real timers/event loops during unit tests or modeling every library.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `registration_not_execution` | Registering a callback adds no immediate callback-body write before a modeled invocation. |
| `promise_branching` | Fulfillment/rejection handlers and later consumers receive only the modeled matching result paths. |
| `combinator_projection` | Audited all-style results preserve element projections; race-style alternatives are not treated as simultaneously certain winners. |
| `capture_after_return` | An escaping captured value continues into a modeled callback beyond the registering function's exit. |
| `unknown_order` | Competing writes under unproved event order remain uncertain rather than selecting one schedule as fact. |
| `fake_scheduler` | Finite deterministic schedule fixtures produce reviewed witnesses without real sleeps or application execution. |
| `changed_callback_flow` | A callback source edit changes its later consumers under the correct execution conditions, not registration-time consumers. |

## Verification

Run `cargo test --locked --offline --lib ifds_k034_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
