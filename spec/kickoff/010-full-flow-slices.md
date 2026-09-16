# K010 — Full slices and value-lifecycle endings

Status: planned. Roadmap stage: 1.

Dependencies: [K008](008-intraprocedural-solver.md), [K009](009-uncertainty-and-limits.md).

Source: [SPEC sections 5 and 6 case 10](../../docs/ifds/SPEC.md).

## Scope

- Build the union of selected writes, upstream inputs, and complete supported
  downstream dependencies; retain unchanged intermediate/final consumers.
- Follow origins through copies independently of the original binding's lifetime.
  Record per-origin/carrier endings, coverage, witnesses, and exact consumer slots.
- Support straight-line no-further-use, entry exit, escaping return, and unknown
  boundaries now; later tasks extend the same contract to controls/cycles/calls.

Excludes including unrelated later statements or mining historical bugfixes.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `complete_copy_chain` | SPEC case 10 retains A-to-B-to-D-to-R despite unchanged consumers and C's overwrite of `x`; C does not feed D/R. |
| `one_copy_survives` | Killing one carrier does not close another carrier's path. |
| `last_carrier_ends` | After all carriers are overwritten with no future control/value uses, record `no_further_use` with the supporting location. |
| `returned_value_escapes` | A tracked entry-function return records `scope_boundary`, not lifecycle closure; an entry exit with no escape is distinguished. |
| `no_sibling_expansion` | An independent use of an upstream input shared by a calculation is not included as a downstream use of selected `x`. |
| `unknown_is_open` | An unsupported continuation preserves the prefix and leaves lifecycle closure false. |
| `sliced_equals_unsliced` | Selected relations match full reference analysis on supported small programs, including unused selected writes. |

## Verification

Run `cargo test --locked --offline --lib ifds_k010_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
