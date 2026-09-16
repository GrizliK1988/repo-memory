# K022 — Audited library/source/sink summaries

Status: planned. Roadmap stage: 5.

Dependencies: [K021](021-read-projections.md).

Source: [SPEC source/sink rules](../../docs/ifds/SPEC.md) and
[PR model manifest](../../docs/ifds/PR-5816.md).

## Scope

- Add a named/versioned summary registry with audited preconditions, transfers,
  evidence labels, precise input projections, and external boundaries.
- Implement required standard `join`, string `split`, literal indexing, preference
  read sources, and request-input sinks under explicit assumptions.
- Distinguish modeled dependencies from supported source-body derivations. Reject
  models when target identity or preconditions cannot be established.

Excludes inventing storage writers, server responses, or purity from a method name.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `join_origins` | A supported string-array join carries element/separator dependencies into the result under its documented preconditions. |
| `split_projection` | Modeled `appLanguage.split("-")[0]` retains the preference origin and exact projection; it does not claim a concrete stored value. |
| `preference_source` | A modeled read identifies its storage key/field and boundary without fabricating earlier persistence writes. |
| `request_sink_only` | A known language request input ends at a modeled boundary; no proven input-to-server-response edge is added. |
| `target_preconditions` | A shadowed/overridden same-named method or unmet coercion precondition cannot use the standard model silently. |
| `model_metadata` | Every modeled relation names its version/assumptions; changing the model set invalidates a mixed-version comparison. |
| `summary_distributivity` | Exhaustive small fact sets verify each IFDS-facing summary transfer distributes over union. |

## Verification

Run `cargo test --locked --offline --lib ifds_k022_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
