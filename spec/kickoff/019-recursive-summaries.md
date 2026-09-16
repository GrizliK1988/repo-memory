# K019 — Recursion and summary evidence

Status: planned. Roadmap stage: 4.

Dependencies: [K018](018-interprocedural-calls.md).

Source: [SPEC recursive scheduling and witness rules](../../docs/ifds/SPEC.md).

## Scope

- Complete direct/mutual recursion through finite summaries and waiting-call
  subscriptions, including new exit facts discovered after a recursive call waits.
- Deduplicate path-edge, waiter, and summary records; do not encode unbounded
  runtime stacks or activation IDs in the fact domain.
- Expand compact summary evidence with matched calls/returns and explicit cycles.

Excludes claiming that fixed-point completion proves concrete recursion terminates.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `direct_recursion` | A reviewed recursive function with a supported base path reaches the expected return origins using finite records. |
| `mutual_recursion` | A/B mutual recursion shares growing summaries and reaches the independent reference fixed point. |
| `late_exit_resumes_waiters` | A newly discovered exit fact resumes all matching waiting calls and no unrelated call. |
| `deduplicate_records` | Repeated entry/exit discoveries do not increase identical waiter or summary counts. |
| `no_base_return` | A recursive cycle with no supported return does not invent normal return facts or execution exit. |
| `witness_pairing` | Expanded evidence cannot return through another call site, including within a recursive SCC. |
| `finite_reference_paths` | On bounded finite examples, reported supported relations match explicit call/return-matched enumeration. |

## Verification

Run `cargo test --locked --offline --lib ifds_k019_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
