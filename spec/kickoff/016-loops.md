# K016 — Loops, break/continue, and cyclic evidence

Status: planned. Roadmap stage: 3.

Dependencies: [K015](015-short-circuit-expressions.md).

Source: [SPEC sections 4–5 and section 6 case 6](../../docs/ifds/SPEC.md).

## Scope

- Lower `while`, `do/while`, classic `for`, nested loops, `break`, and `continue`.
  Reuse static definitions through the existing finite fixed-point solver.
- Preserve zero-iteration and loop-carried paths, guards, and compact cyclic
  witnesses; solver convergence does not establish runtime termination.
- Extend flow comparison and lifecycle coverage to backedges and loop exits.

Excludes fixed unrolling as the analysis algorithm and iterator protocols from K035.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `while_vs_do` | A zero-iteration while path retains the pre-loop writer; a do/while body executes before its first condition check. |
| `for_continue_target` | `continue` in a for loop reaches its update then condition; a while continue reaches its condition. |
| `nested_break` | An unlabeled break exits only the innermost loop; outer flow and writers remain correct. |
| `loop_carried_kill` | Reviewed cyclic fixtures retain real carried origins but no definition killed before every read. |
| `insert_loop_write` | Case 6 adds B-to-R under entry flag and narrows retained A-to-R to its negation; A-to-R is not removed. |
| `finite_evidence` | Repeated iterations reuse IDs and compact cycle witnesses; no unbounded path-string growth. |
| `cycle_not_exit` | A supported continuing cycle is recorded as such, not `execution_exit`; unknown termination remains explicit. |

## Verification

Run `cargo test --locked --offline --lib ifds_k016_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
