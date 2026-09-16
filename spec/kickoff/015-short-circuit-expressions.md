# K015 — Conditional and short-circuit expressions

Status: planned. Roadmap stage: 2.

Dependencies: [K014](014-branches.md).

Source: [roadmap stage 2](../../docs/ifds/README.md) and
[SPEC evaluation-order rules](../../docs/ifds/SPEC.md).

## Scope

- Lower ternaries and `&&`, `||`, `??` into ordered branches with value-producing
  joins; preserve exact operand inputs and path conditions.
- Distinguish truthiness from nullishness. Unsupported coercions/side effects
  create boundaries only on paths where that operand can run.
- Compare changed operands/guards without treating skipped expressions as writes.

Excludes logical assignment operators, owned by K027.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `ternary_sources` | `x=flag?a:b` reaches later reads from the corresponding branch origins only. |
| `and_or_skip_rhs` | Known false `&&` and known true `\|\|` skip an RHS assignment/effect; opposite outcomes evaluate it once. |
| `nullish_not_falsy` | `0`, false and empty string do not select a `??` fallback; null and undefined do under explicit entry facts. |
| `nested_order` | Nested supported conditional expressions produce the reviewed evaluation order and no skipped operand facts. |
| `operand_change_delta` | Editing one ternary source affects only its conditioned downstream paths; the other arm is retained. |
| `unknown_rhs_boundary` | An unknown RHS operation remains an open boundary on the evaluating path, not on a proven skipped path. |

## Verification

Run `cargo test --locked --offline --lib ifds_k015_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
