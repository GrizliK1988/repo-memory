# K029 — Switch and labeled control transfers

Status: planned. Roadmap stage: 8.

Dependencies: [K028](028-binding-and-parameter-forms.md), [K016](016-loops.md),
[K023](023-exceptions.md).

Source: [roadmap stage 8](../../docs/ifds/README.md).

## Scope

- Lower switch matching, case evaluation, default selection, and fallthrough.
  Resolve labeled break/continue targets through nested supported constructs.
- Preserve guards, reaching definitions, and finally handling on abrupt transfers.
- Extend changed-flow and negative-path fixtures rather than merely adding AST support.

Excludes unsupported case-expression effects or unproved value comparisons being assumed safe.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `switch_fallthrough` | A matched case without break reaches later case statements and their writes; a break prevents that fallthrough. |
| `default_selection` | Default is selected only when no supported case matches; its source location does not imply it always executes last. |
| `case_evaluation_order` | Side effects of supported case expressions occur in the correct search order, not for every later case unconditionally. |
| `labeled_break` | A labeled break reaches the identified enclosing target, preserving unrelated inner/outer paths. |
| `labeled_continue` | A labeled continue reaches the target loop's correct update/condition, including required finally work. |
| `removed_break_delta` | Removing a break adds only newly reachable fallthrough writer/consumer paths with the right conditions. |
| `unknown_case_guard` | Unsupported matching semantics yield unresolved feasibility rather than supported contradictory flows. |

## Verification

Run `cargo test --locked --offline --lib ifds_k029_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
