# K014 — Branches, early returns, and control influence

Status: done. Roadmap stage: 2.

Dependencies: [K013](013-analysis-api.md).

Source: [SPEC sections 4–5 and section 6 case 5](../../docs/ifds/SPEC.md).

## Scope

- Lower `if`/`else`, nested branches, joins, and early returns; carry guard outcomes
  into typed flow evidence and per-path lifecycle records.
- Add constant-condition pruning and basic boolean guard checks now. Unsupported
  feasibility remains unknown rather than being deferred as false certainty.
- Follow control consequences separately from value copies; compare conditions
  on retained relations without manufacturing new origins or writes.

Excludes a general predicate solver and claims that all CFG paths are executable.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `one_sided_write` | `x=0(A); if(flag)x=1(B); return x` has A under !flag and B under flag; no join writer. |
| `two_sided_nested` | Reviewed nested if/else fixtures have exactly the admissible writer/guard combinations. |
| `early_return` | A returned branch cannot reach later assignments; the other branch continues with its own source set. |
| `guard_only_delta` | Case 5 retains both writers and reaching edges, changes conditions, and adds no value-source delta at R. |
| `control_not_copy` | `if(x>0)y=1; return y` shows the control step from x to the conditional write and later y use, not a copied x value into literal 1. |
| `false_and_contradictory` | Constant-false and supported contradictory guards cannot produce supported positive paths; unsupported guard theories produce explicit uncertainty. |
| `path_endings_independent` | A closed branch does not close another branch's escaping or unknown continuation. |

## Verification

Run `cargo test --locked --offline --lib ifds_k014_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
