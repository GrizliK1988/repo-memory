# K023 — Exceptions and finally completion

Status: planned. Roadmap stage: 6.

Dependencies: [K020](020-caller-continuations.md).

Source: [roadmap stage 6](../../docs/ifds/README.md) and
[benchmark exception cases](../../docs/ifds/BENCHMARKS.md).

## Scope

- Model `throw`, try/catch/finally, exceptional call exits, and pending completion
  kinds for normal return, throw, break, and continue.
- Preserve evaluation order: an exception before RHS completion prevents assignment
  commit. Finally executes before the pending transfer and may replace it.
- Keep exceptional/normal witnesses and lifecycle endings distinct.

Excludes silently treating unknown throwing behavior as a guaranteed normal return.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `throw_before_commit` | A throwing RHS does not write the target; the previous definition reaches the supported catch read. |
| `catch_overwrite` | A catch-side assignment replaces sources only on exceptional paths. |
| `finally_before_transfer` | Return, throw, break, and continue pass through the correct finally operations before their destinations. |
| `finally_overrides` | A return/throw from finally overrides the pending completion; replaced destinations get no false flow. |
| `evaluated_return_value` | A return expression evaluated before finally retains that value when finally only reassigns its source binding. |
| `exceptional_call_pairing` | Exceptions resume the correct caller handler, never another call's normal return site. |
| `unknown_throw_boundary` | Unmodeled exceptions keep affected paths partial; lifecycle closure is not asserted across them. |

## Verification

Run `cargo test --locked --offline --lib ifds_k023_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
