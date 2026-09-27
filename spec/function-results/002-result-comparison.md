# FR002 — Conditions and result comparison

Status: planned.

Dependencies: [FR001](001-function-query-and-slice.md),
[K011](../kickoff/011-node-alignment.md), [K012](../kickoff/012-flow-comparison.md).

Source: [function result specification sections 4 and 6](../../docs/ifds/FUNCTION_RESULTS.md).

Fixture examples: [FR002 acceptance test sketches](TEST_EXAMPLES.md).

## Scope

- Compare guarded summaries on paired equal inputs and feasible intersections of
  before/after conditions. Record common-domain coverage and excluded inputs.
- Implement the minimal declared Boolean and primitive result theory separately
  from IFDS transfers; retain symbolic expressions outside that theory.
- Emit `equal`, `different`, `changed`, or `unknown` per region with evidence/proof references.
  Keep structural, control, source, and result changes separately identifiable.
- Track source-alignment certainty separately from result-comparison certainty.
  Preserve known regions when other regions or input mappings are unresolved.

Excludes universal expression equivalence, general numeric solving, whole-program
behavioral equivalence, and claims that a concrete witness proves a larger region.

## Required unit tests

| Test suffix | Required evidence |
| --- | --- |
| `guard_added` | The enabled/ready example yields exactly the three specified regions; only enabled and not ready has different results despite unchanged possible result values. |
| `equal_branch_values` | Guard inversion with two constant-1 returns changes control but proves equal normal results. |
| `early_return_and_undefined` | Inserting an early return changes only its input region; adding a final return compares implicit undefined with its new value. |
| `return_shape` | Ternary versus two return sites proves equal results independently of uncertain site alignment; attribution uncertainty remains visible. |
| `same_input_and_copy` | Equal paired primitive inputs and copies prove equal; returning distinct paired unknown inputs p/q reports `changed` without claiming unequal values. |
| `expression_changed` | `return p + 1` -> `return p + 2` reports `changed` without numeric evaluation or an unequal-value claim; the unchanged pure expression over the same paired primitive input can report `equal`. |
| `guard_versions_and_order` | Reassigned guards and reordered conditional overwrites retain their actual result selection; text-equal guards are not merged across versions. |
| `guard_changes_return` | Changing `if (flag)` to `if (!flag)` preserves both return sites but swaps which value is returned for each symbolic Boolean input. Report the condition edit and its return consequence without evaluating the guard. Do not compare mutually exclusive same-literal paths as a common input region. |
| `common_domain` | Different entry assumptions disclose their common domain and excluded regions; missing correspondence prevents affected result claims. |
| `independent_unknown_region` | A supported changed branch remains reportable beside an unknown branch without claiming whole-query equality or complete changed coverage. |
| `presence_not_value` | Confirmed function addition/removal is a presence finding, not a comparison against fabricated undefined; ambiguous function matching stays unresolved. |
| `primitive_semantics` | Supported ordinary primitive literals compare by the declared result-equality rules; coercions, unmodeled special numbers, and object identity stay unknown. |
| `declared_input_scope` | A result established for the declared input domain `flag = true` is scoped to that domain; no result claim is made for excluded `flag = false` inputs. |

## Verification

Run `cargo test --locked --offline --lib ifds_fr002_ -- --list`, then
`cargo test --locked --offline --lib ifds_fr002_` and the
[feature completion gate](README.md#completion-gate).
Review result regions against independently authored input tables and proofs.
