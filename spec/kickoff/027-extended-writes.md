# K027 — Compound and destructuring writes

Status: planned. Roadmap stage: 7.

Dependencies: [K026](026-heap-and-aliases.md), [K015](015-short-circuit-expressions.md).

Source: [roadmap stage 7](../../docs/ifds/README.md) and
[SPEC ordered evaluation rules](../../docs/ifds/SPEC.md).

## Scope

- Lower supported compound assignments, prefix/postfix updates, logical assignments,
  and object/array destructuring writes into ordered reads, computes, and commits.
- Evaluate targets/RHS/defaults in language order, retaining exact property paths
  and conditional writes. Reuse scalar/heap transfer semantics rather than adding
  whole-fact-set rules.

Excludes unmodeled getter/coercion/iterator effects; expose those boundaries explicitly.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `compound_reads_old_value` | `x += y` includes old x and y origins before the new x write. |
| `prefix_postfix_result` | `a=x++` uses old x for a; `a=++x` uses updated x, with exactly one update write. |
| `logical_assignment_skip` | `&&=`, `\|\|=`, and `??=` commit only on their respective supported truthy/falsy/nullish paths. |
| `destructuring_projection` | `{a:x,b:y}=source` maps distinct field origins to x/y; array positions remain distinct. |
| `default_and_order` | Defaults run only for undefined inputs; side-effecting target/RHS subexpressions are evaluated in the required order and count. |
| `partial_commit_exception` | A later throwing destructuring step preserves earlier completed writes but does not commit later ones. |
| `changed_target_delta` | Retargeting one destructured field changes only its selected write/dependencies; sibling writes stay retained. |

## Verification

Run `cargo test --locked --offline --lib ifds_k027_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
