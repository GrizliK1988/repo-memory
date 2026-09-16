# K028 — Hoisting, TDZ, and parameter forms

Status: planned. Roadmap stage: 8.

Dependencies: [K027](027-extended-writes.md), [K018](018-interprocedural-calls.md).

Source: [roadmap stage 8](../../docs/ifds/README.md) and
[benchmark binding cases](../../docs/ifds/BENCHMARKS.md).

## Scope

- Extend binding/evaluation models to `var` hoisting, uninitialized locals,
  temporal dead zones, and destructured/default/rest parameters.
- Distinguish undefined/uninitialized states from the internal Zero fact; retain
  lexical scope identity and ordered parameter initialization.
- Ignore type-only annotations as runtime writes; diagnose unsupported runtime
  TypeScript constructs rather than mistaking them for erased type syntax.

Excludes a complete TypeScript type checker or type-driven value/purity guarantees.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `var_before_initializer` | A read of hoisted var before its initializer has the reviewed undefined-entry source, not the later assignment. |
| `tdz_read` | A pre-initialization let/const read follows the appropriate error path rather than receiving Zero or undefined as its value. |
| `hoisted_scope_identity` | Var and block-scoped declarations resolve according to their distinct scopes and shadowing rules. |
| `default_parameters` | Undefined arguments evaluate defaults; explicit null does not; earlier initialized parameters can feed later defaults. |
| `destructured_and_rest` | Parameter fields/rest elements retain exact actual-argument sources and indices. |
| `type_only_change` | Editing an annotation creates no runtime writer or value-flow delta when executable semantics are unchanged. |
| `unsupported_runtime_form` | A relevant unsupported runtime construct emits its frontier instead of being erased as type-only syntax. |

## Verification

Run `cargo test --locked --offline --lib ifds_k028_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
