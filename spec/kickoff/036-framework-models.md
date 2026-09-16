# K036 — Audited framework state/effect models

Status: planned. Roadmap stage: 10.

Dependencies: [K034](034-deferred-execution.md), [K031](031-classes.md).

Source: [roadmap stage 10](../../docs/ifds/README.md).

## Scope

- Define versioned, separately tested framework summaries, starting with a declared
  minimal React state/effect subset relevant to the application fixtures.
- Distinguish render snapshots, scheduled state updates, effect execution/cleanup,
  and ordinary assignments. Record framework/version/scheduling assumptions.
- Leave unsupported hooks, render ordering, and framework behavior as boundaries;
  add other framework families only with their own model/test manifest.

Excludes treating React conventions as language-independent IFDS semantics.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `state_update_not_assignment` | A modeled setter schedules an update rather than reassigning the current render's scalar binding. |
| `render_snapshot` | A callback captures the modeled render's value; no automatic replacement with a later render's source. |
| `effect_registration` | Registering an effect is distinct from running its body; supported scheduling assumptions appear in witnesses. |
| `effect_cleanup` | A declared cleanup model produces only its supported ordering/effects; unproved timing remains unknown. |
| `version_and_identity` | Wrong framework version or a shadowed hook name prevents silent application of the standard summary. |
| `unsupported_hook` | An unmodeled hook produces an explicit boundary, not no-op behavior. |
| `changed_state_source` | A changed modeled update source propagates to the correct later consumers without rewriting prior render origins. |

## Verification

Run `cargo test --locked --offline --lib ifds_k036_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
Use synthetic versioned framework fixtures; no live UI/runtime is required to pass unit tests.
