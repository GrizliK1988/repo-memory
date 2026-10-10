# K016 — Loops, break/continue, and cyclic evidence

Status: done. Roadmap stage: 3.

Dependencies: [K015](015-short-circuit-expressions.md).

Source: [SPEC sections 4–5 and section 6 case 6](../../docs/ifds/SPEC.md).

## Scope

- Lower `while`, `do/while`, classic `for`, nested loops, `break`, and `continue`.
  Reuse static definitions through the existing finite fixed-point solver.
- Preserve zero-iteration and loop-carried paths, guards, and compact cyclic
  witnesses; solver convergence does not establish runtime termination.
- Extend flow comparison and lifecycle coverage to backedges and loop exits.

Excludes fixed unrolling as the analysis algorithm and iterator protocols from K035.

## Implementation decisions

- Represent each loop by a finite, source-mapped structure: condition, body and
  update operations, input bindings and origins, carried values, and exits. IDs
  are static; an operation is not cloned per iteration. Execution counts need
  not be established. Reports distinguish possibly skipped bodies, possibly
  repeated writes, and proven unreachable operations.
- Preserve the initial guard's value separately from reevaluation after a write.
  Finite abstract states may establish a one-shot Boolean loop; changing or
  unsupported repeated tests retain possible flows with explicit uncertainty.
  Never collapse entry and exit tests into a contradictory conjunction.
- A backedge is not itself proof of another body execution or runtime
  nontermination. Record compact cycles only for continuing abstract paths;
  convergence is not proof of a normal return or an exact accumulated value.
- Classic `for` uses the already supported scalar assignment subset, including
  `i = i + 1`. Updates such as `i++` and `+=` remain K027 boundaries. Labeled
  transfers remain K029 boundaries; `for...of` and `forEach` are not K016 loops.
- Compare static operations, dependencies and guards across snapshots. Loop
  syntax changes do not alone prove different returned values. Unsupported new
  syntax preserves the known before graph and an explicit after boundary.
- Both binding and function reports expose loop parameters and possible repeated
  operations. Exact result claims require established return selection and
  completion. Independent supported regions survive an uncertain loop region.

Additional acceptance covers function returns, inserted/removed loops, changed
exit conditions, unsupported updates/iteration, nested scopes, and report evidence.

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

Additional K016 tests cover `for_scope_and_unreachable_body`,
`budget_keeps_loop_effects_uncertain`, `function_insert_remove_and_return`,
`function_changed_exit_and_unknown_region`,
`function_loop_forms_and_unsupported_iteration`,
`function_accumulation_reports_structure`, `repeated_value_join_uses_latest_arm`,
and `loop_entry_scope_and_call_boundary`.

## Delivery

- `ProcedureIr.loops` records static CFG structure. `LoopSummary` supplies
  source-mapped inputs/origins, carried bindings, operations and exits to both
  report targets. The initial guard's read definitions and choices are captured
  separately from current value-join choices.
- Path validation follows backedges to finite abstract states. Strongly connected
  continuing state components provide compact cycle witnesses; a one-shot loop
  with a backedge and a known false reevaluation is not classified as repeating.
- Binding report schemas are full 4 / compact 3. Function report schemas are
  analysis 5 / full 6 / compact 6. Archived reports retain their original schemas.
- [Actual reports](016-loop-reports/README.md) show inserted loops, repeated
  accumulation and unsupported callback iteration. Exact expectations are authored
  in the Rust tests; these generated artifacts are review examples, not oracles.
- Verification: all 15 `ifds_k016_` tests were discovered and passed; the shared
  offline gate passed with 276 library tests, 9 integration tests and 2 doctests,
  plus formatting and clippy with warnings denied.

## Verification

Run `cargo test --locked --offline --lib ifds_k016_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
