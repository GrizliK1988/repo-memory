# K032 — Bounded path-feasibility refinement

Status: planned. Roadmap stage: 9.

Dependencies: [K029](029-switch-and-labels.md), [K031](031-classes.md).

Source: [SPEC sections 6.2 and 7](../../docs/ifds/SPEC.md).

## Scope

- Extend guard reasoning with a declared finite theory and bounded witness
  validation, separate from distributive IFDS transfers.
- Consider alternative witnesses before rejecting a candidate relation. Distinguish
  proven impossible, established possible, and unresolved feasibility.
- Compare equivalent/different conditions semantically within the supported theory;
  timeouts or theory limits remain unknown, never a guessed flow delta.

Excludes universal TypeScript satisfiability or numeric per-finding confidence scores.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `contradictory_guards` | A supported contradictory guard combination cannot produce a supported relation. |
| `alternative_witness` | Rejecting one impossible path does not remove a relation with another valid matched path. |
| `equivalent_conditions` | Different guard syntax proved equivalent produces no `FlowConditionChanged`. |
| `proved_difference` | A supported changed condition yields before/after conditions on the retained relation, not removed/added edges. |
| `validation_budget` | Exhausting a fake validation budget gives `UnprovenPathFeasibility`/limit evidence, not absent flow. |
| `unknown_theory` | Unsupported operations in a guard keep affected claims unresolved while known unrelated relations survive. |
| `transfers_unchanged` | All finite transfer distributivity checks still pass; feasibility does not depend on a hidden conjunctive fact transfer. |

## Verification

Run `cargo test --locked --offline --lib ifds_k032_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
