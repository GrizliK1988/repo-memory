# K007 — Reaching definitions and origin transfers

Status: planned. Roadmap stage: 1.

Dependencies: [K001](001-contracts.md).

Source: [SPEC section 4, fact families and applyWrite](../../docs/ifds/SPEC.md).

## Scope

- Implement finite `Zero`, `LastWrite`, and `Origin` facts and distributive
  singleton transfer functions for supported writes/computations.
- Generate new writer/origin facts through Zero, kill the overwritten place's old
  facts, preserve other places, and copy already-evaluated input origins.
- Seed unknown function parameters as labeled entry sources. Store dependency
  evidence separately from fact-set semantics.

Excludes alias discovery and whole-set predicate reasoning inside transfer functions.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `zero_generates_literal` | A reachable literal write creates its writer/origin and preserves Zero; empty input creates no facts. |
| `overwrite_kills_one_place` | Reassigning `x` removes its old LastWrite/Origin facts without removing facts for `y`. |
| `copy_survives_overwrite` | `x=10(A); y=x(B); x=20(C)` leaves y's last writer B and origins A/B, never C. |
| `self_assignment_origins` | A temporary holding old `x` feeds `x=x+1`; old origins survive alongside the new write. |
| `parameter_seeding` | Each parameter gets its own entry writer and input origin, not another parameter's source. |
| `distributivity` | Exhaustively enumerate small fact sets A/B for every transfer: `f(A union B) = f(A) union f(B)`. |
| `finite_static_ids` | Reapplying a write does not allocate new iteration/activation source IDs. |

## Verification

Run `cargo test --locked --offline --lib ifds_k007_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
