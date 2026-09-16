# K037 — Module initialization and dynamic imports

Status: planned. Roadmap stage: 10.

Dependencies: [K017](017-project-metadata.md), [K023](023-exceptions.md),
[K034](034-deferred-execution.md).

Source: [roadmap stage 10](../../docs/ifds/README.md).

## Scope

- Extend name/module resolution into supported module initialization order, live
  bindings, dependency cycles, and resolvable dynamic import continuations.
- Respect the declared runtime/module configuration and finite module identities.
  Initialization failures and unsupported asynchronous/cyclic behavior are explicit.
- Keep source loading/resolution separate from executing application code.

Excludes running module bodies in the analyzer process to discover their effects.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `initialization_order` | A supported acyclic module graph exposes writes to consumers in the reviewed dependency order. |
| `live_binding` | A supported exported binding update reaches importing reads without being treated as an unconditional import-time scalar copy. |
| `cyclic_modules` | A reviewed cycle converges with finite IDs; reads before initialization follow the modeled TDZ/uncertainty path, not fabricated values. |
| `known_dynamic_import` | A known target connects its fulfilled module projection and later consumer under the matching continuation. |
| `unknown_dynamic_import` | Unknown targets or unsupported runtime mode produce explicit resolution/schedule boundaries. |
| `initialization_failure` | A failing initializer follows the exceptional import path, not a successful normal value read. |
| `no_source_execution` | A recording provider observes only metadata/source reads; no imported source code or package script executes. |

## Verification

Run `cargo test --locked --offline --lib ifds_k037_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
