# K030 — Closures and synchronous function values

Status: planned. Roadmap stage: 8.

Dependencies: [K028](028-binding-and-parameter-forms.md),
[K020](020-caller-continuations.md), [K026](026-heap-and-aliases.md).

Source: [roadmap stage 8](../../docs/ifds/README.md) and
[SPEC lifetime boundaries](../../docs/ifds/SPEC.md).

## Scope

- Resolve supported function-valued locals and synchronous callbacks; model
  lexical captures, captured writes, per-iteration bindings, and arrow `this`.
- Distinguish a captured binding from an earlier copied scalar value.
- Follow values escaping through captures only under a supported invocation model;
  stored/deferred callbacks remain explicit boundaries until K034.

Excludes treating callback registration as synchronous execution.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `captured_reassignment` | A synchronously invoked closure's write reaches the correct captured binding, not a same-named local. |
| `capture_vs_copy` | A closure reads the supported current capture at invocation; a separate earlier scalar copy keeps its earlier origin. |
| `per_iteration_binding` | Loop let captures distinguish per-iteration environments without unbounded static IDs; var capture follows its shared binding model. |
| `resolved_function_value` | A known function-valued alias has a matched call/return path; multiple unresolved targets remain uncertain. |
| `arrow_lexical_this` | An arrow preserves its lexical receiver rather than acquiring the call-site receiver. |
| `stored_callback_boundary` | Merely storing/registering a callback does not execute its writes; escaping capture leaves an open continuation. |
| `capture_delta` | Changing a captured source updates unchanged later callback consumers only on supported invocation paths. |

## Verification

Run `cargo test --locked --offline --lib ifds_k030_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
