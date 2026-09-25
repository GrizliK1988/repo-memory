# K015 — Conditional and short-circuit expressions

Status: done. Roadmap stage: 2.

Dependencies: [K014](014-branches.md).

Source: [roadmap stage 2](../../docs/ifds/README.md) and
[SPEC evaluation-order rules](../../docs/ifds/SPEC.md).

## Scope

- Lower ternaries and `&&`, `||`, `??` into ordered branches with a value-producing
  join whose result origins come from the selected arm. Preserve operand input
  roles, source spans, evaluation order, and path conditions.
- Distinguish JavaScript truthiness from nullishness. Unsupported value tests or
  effects remain uncertain on the paths that can evaluate them.
- Compare changed operands and guards without treating skipped expressions as
  reads, effects, or writes.

Excludes logical assignment operators (`&&=`, `||=`, `??=`), compound/update
assignments, destructuring, and heap writes, owned by K027.

## Semantics and implementation boundary

- Evaluate the condition or left operand once, then choose an edge. `c ? a : b`
  evaluates only `a` when `c` is truthy and only `b` otherwise. `a && b`
  returns `a` when it is falsy and otherwise evaluates and returns `b`; `a || b`
  returns `a` when it is truthy and otherwise evaluates and returns `b`; `a ?? b`
  returns `a` unless it is null or undefined, and otherwise evaluates and returns
  `b`. These operators return operand values, not converted booleans.
- Capture the tested operand in a temporary before entering a branch. Each arm
  supplies its selected, already evaluated value to a join-owned result temporary;
  the join merges those alternatives without creating a source-level write. The
  IR must give that result one producer and transfer only origins available on
  the incoming path. An enclosing assignment commits once on each continuing
  path, after its RHS has finished. A guard controls the choice but is a value
  origin only when the guard value is itself the selected operand.
- At minimum, decide truthiness for supported literal booleans, null,
  undefined, numeric zero and nonzero literals, empty and nonempty string
  literals, and their copies. Decide nullishness for these literals and copies:
  only null and undefined are nullish. Keep truthiness and nullishness as
  distinct branch tests; a false, zero, or empty string must not take a `??`
  fallback. Values without a supported proof take both possible edges with
  explicit unproven feasibility. A branch condition must never be inferred from
  origin facts alone. Entry-value assumptions used by tests must be explicit.
- Support plain scalar `identifier = expression` when it occurs in an expression
  arm: evaluate the RHS, commit the one binding write, and return the assigned
  value. This is the minimum side effect needed to verify short-circuiting; it
  does not extend K015 to the assignment forms excluded above. A call or other
  unsupported operation in an arm keeps its existing unknown boundary only on
  paths that can evaluate that arm. A proven skipped arm contributes no executed
  read, effect, write, or unknown boundary.
- Nested expressions use the same rules recursively, including inside an `if`
  condition, a return, and an assignment RHS. Preserve distinct branch decisions
  and source-mapped conditions through the join. Do not claim that an unknown
  combination of guards is feasible or impossible beyond K014's supported guard
  rules; broader predicate reasoning belongs to K032.
- Across snapshots, compare each retained relation and its conditioned origins
  separately. A changed guard with the same possible origins changes supported
  conditions, not value sources. A changed selected arm can change the consumer's
  source set only on paths that select that arm. A proven skipped write is absent;
  an uncertain arm is not treated as absent.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `ternary_sources` | `x=flag?a:b` has one static outer write that executes once per continuing path; later reads have `a` origins under `flag` and `b` origins under `!flag`, with no join writer or copied `flag` origin. |
| `and_or_skip_rhs` | In `false && (x=1)` and `true \|\| (x=1)`, the RHS write is unreachable; opposite known outcomes execute it exactly once. The expression result is the selected operand value, including a falsy left operand. |
| `nullish_not_falsy` | Explicit known values `0`, false, and `""` retain the left operand of `??`; null and undefined select its fallback. Unknown entry values retain both conditioned alternatives without a false certainty claim. |
| `nested_order` | Nested expressions, including one in an `if` condition and one in an assignment RHS, evaluate each reachable operand once in source order; skipped operands contribute no read or fact. |
| `operand_change_delta` | Editing one ternary arm changes only its selected downstream value sources; the other arm's relation is retained. A guard-only edit with unchanged possible origins changes supported conditions, not value sources. |
| `unknown_rhs_boundary` | A call or other unsupported RHS operation is an open boundary on evaluating paths; a proven skipped path has no executed unknown boundary, while an unresolved test preserves uncertainty. |

## Verification

Run `cargo test --locked --offline --lib ifds_k015_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
