# K002 — Immutable snapshots and query validation

Status: done. Roadmap stage: 0.

Dependencies: [K001](001-contracts.md).

Source: [SPEC section 3](../../docs/ifds/SPEC.md).

## Scope

- Provide read-only snapshot access, validated repository diffs/rename metadata,
  content identities, and one-binding selectors with optional explicit counterparts
  and enclosing-symbol/name assertions.
- Validate revision hashes and coordinates before analysis, preserve selector
  assertions for K005's declaration-resolution check, and represent default entry
  and later caller-context options without executing source code.
- Provide in-memory providers for tests and a Git provider that reads immutable
  objects without checkout, reset, hooks, or application/package-script execution.

Excludes lexical declaration resolution, owned by K005.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `matching_diff` | A valid edit maps the supplied before bytes to after bytes; mismatched content/rename mappings are rejected. |
| `one_binding_only` | Missing or multiple selectors are rejected; a valid one-sided selector is accepted for later counterpart resolution. |
| `stale_revision` | A selector hash from another snapshot fails before any solver call. |
| `selector_assertion_contract` | Optional declared-name and enclosing-symbol assertions survive query validation and round-trip unchanged for declaration resolution; omitted assertions remain optional rather than becoming guessed names. |
| `utf8_coordinates` | A multibyte identifier's byte span is preserved; out-of-bounds and mid-codepoint spans fail. |
| `versions_match` | Before/after capability or model versions differ; reject comparison rather than reporting software-version effects as source deltas. |
| `no_workspace_mutation` | A recording provider exposes reads only; analysis setup neither writes files nor requests application execution. |

## Verification

Run `cargo test --locked --offline --lib ifds_k002_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
Also test the real Git provider against a temporary local repository: read two
commits while an unrelated dirty worktree remains byte-for-byte unchanged.
