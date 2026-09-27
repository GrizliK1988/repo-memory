# FR002 — Conditions and result comparison

Status: implemented for the first subset, including symbolic truthiness of
unrestricted paired inputs in pure guards.

Dependencies: [FR001](001-function-query-and-slice.md),
[K011](../kickoff/011-node-alignment.md), [K012](../kickoff/012-flow-comparison.md).

Source: [function result specification sections 4 and 6](../../docs/ifds/FUNCTION_RESULTS.md).

Fixture examples: [FR002 acceptance test sketches](TEST_EXAMPLES.md).

## Scope

- Compare guarded summaries on paired equal inputs and feasible intersections of
  before/after conditions and entry-assumption expressions. Record common-domain
  coverage, one-sided domains, excluded inputs, and changes to declared scope.
- Implement the minimal JavaScript truthiness and primitive result theory
  separately from IFDS transfers. Pure guards over stable paired positional
  inputs work without declared Boolean domains; retain symbolic expressions
  outside that theory.
- Emit `equal`, `different`, `changed`, or `unknown` per region with evidence/proof references.
  Keep structural, control, source, and result changes separately identifiable.
- Track source-alignment certainty separately from result-comparison certainty.
  Preserve known regions when other regions or input mappings are unresolved.

## Decisions for the first delivery

- A result relation requires a feasible input shared by both declared entry
  domains. When the intersection is empty, report `no common inputs` rather
  than a vacuous `equal` relation. Keep each side's guarded normal-result flow
  under its own entry assumptions, including its conditions, result expressions,
  dependencies, and completion uncertainty. Do not pair the two flows as a
  same-input result comparison or invent an absent-side result.
- An unresolved call or effect before a return makes the affected region's
  completion unknown. Matching return expressions on the continuing paths may
  be retained as evidence, but the region's result relation is `unknown` until
  normal completion is established on both sides.
- Identical unsupported predicates in both snapshots do not establish a common
  branch. Their feasible intersection stays unresolved until purity and paired
  input identity are established by a supported theory or named model.
- A changed argument to an unresolved call is a dependency finding, not a
  proved change to that call's returned computation. For example,
  `return mystery(p)` -> `return mystery(q)` has an `unknown` result relation.
- A declared `number` input domain includes all JavaScript number values,
  including `NaN`, infinities, and signed zero. Until special values are modeled,
  direct comparisons involving them are unknown. Structural identity of the
  same supported pure deterministic expression over paired primitive inputs
  may establish equality without evaluating either side; that proof uses the
  declared SameValue result rule and must preserve identical operation semantics.
- Truthiness result regions are disjoint. Combine regions only when the same
  relation and result-selection rule are established over their union. Retain
  distinct observations and evidence references even when their result rule
  combines into one region.
- For pure reads of the same paired positional input, `if (x)` and `if (!x)`
  partition its truthy and falsy values without a declared type. Supported
  conjunction/disjunction of such reads uses the same symbolic truthiness.
  Keep a condition edit visible even when its return effect is equal; claim a
  changed result only where both normal returns and the shared input region are
  established. Effects or unstable reads remain unresolved.

Excludes universal expression equivalence, general numeric solving, whole-program
behavioral equivalence, and claims that a concrete witness proves a larger region.

## Required unit tests

| Test suffix | Required evidence |
| --- | --- |
| `guard_added` | With no Boolean entry domain, the enabled/ready example yields exactly the three specified truthiness regions; only truthy enabled and falsy ready has different results despite unchanged possible result values. |
| `equal_branch_values` | Guard inversion with two constant-1 returns changes control but proves equal normal results. |
| `early_return_and_undefined` | Inserting an early return changes only its input region; adding a final return compares implicit undefined with its new value. |
| `return_shape` | Ternary versus two return sites proves equal results independently of uncertain site alignment; attribution uncertainty remains visible. |
| `same_input_and_copy` | Equal paired primitive inputs and copies prove equal; returning distinct paired unknown inputs p/q reports `changed` without claiming unequal values. |
| `expression_changed` | `return p + 1` -> `return p + 2` reports `changed` without numeric evaluation or an unequal-value claim; the unchanged pure expression over the same paired primitive input can report `equal`. |
| `guard_versions_and_order` | Reassigned guards and reordered conditional overwrites retain their actual result selection; text-equal guards are not merged across versions. |
| `guard_changes_return` | With no Boolean entry domain, changing `if (flag)` to `if (!flag)` preserves both return sites but swaps which value is returned for each paired input's truthiness. Report the condition edit and its return consequence without evaluating the guard. Do not compare mutually exclusive same-literal paths as a common input region. |
| `common_domain` | Different entry assumptions disclose their common domain and excluded regions; missing correspondence prevents affected result claims. |
| `independent_unknown_region` | A supported changed branch remains reportable beside an unknown branch without claiming whole-query equality or complete changed coverage. |
| `presence_not_value` | Confirmed function addition/removal is a presence finding, not a comparison against fabricated undefined; ambiguous function matching stays unresolved. |
| `primitive_semantics` | Supported ordinary primitive literals compare by the declared result-equality rules; coercions, unmodeled special numbers, and object identity stay unknown. |
| `declared_input_scope` | A result established for the declared input domain `flag = true` is scoped to that domain; no result claim is made for excluded `flag = false` inputs. |
| `entry_assumption_change` | Different before/after Boolean expressions over paired positional inputs are compared as query-scope conditions. Equal results are claimed only on their proven common domain; one-sided or unresolved domains are disclosed, and an assumption edit is not attributed to a source-code edit. |
| `empty_common_domain` | Incompatible entry assumptions yield `no common inputs` and no cross-version result relation; both sides' guarded result flows remain visible under their own domains. |

## Verification

Run `cargo test --locked --offline --lib ifds_fr002_ -- --list`, then
`cargo test --locked --offline --lib ifds_fr002_` and the
[feature completion gate](README.md#completion-gate).
Review result regions against independently authored input tables and proofs.
