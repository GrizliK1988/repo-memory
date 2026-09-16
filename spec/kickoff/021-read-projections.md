# K021 — Read projections and known spreads

Status: planned. Roadmap stage: 5.

Dependencies: [K020](020-caller-continuations.md).

Source: [SPEC section 5](../../docs/ifds/SPEC.md) and
[PR input distinctions](../../docs/ifds/PR-5816.md).

## Scope

- Model literal plain objects/arrays, known property/index reads, bounded read
  paths, call-argument projections, and closed known object spreads.
- Preserve sibling-field separation and source-order overwrites. Track exact slots
  such as `arguments[1].headers["Accept-Language"]` rather than the whole options object.
- Unknown keys, getters, spreads, or access-path limits remain explicit boundaries.

Excludes general heap writes/aliases and assuming every property read is side-effect-free.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `sibling_fields` | `{lang:x, label:y}` sends x only into the lang projection, never the label sibling. |
| `literal_array_index` | A known index reads its element's origins; an unresolved index reports only the supported alternatives/uncertainty. |
| `exact_request_inputs` | x in URL interpolation and Accept-Language creates two distinct consumer slots, not one generic fetch-options claim. |
| `disjoint_spread` | A closed labeler-only spread cannot overwrite the language field and creates no language origin. |
| `ordered_overwrite` | A later known same-key property/spread replaces the earlier source for that property only. |
| `unknown_projection` | Unknown spread/getter/key prevents an unjustified complete source set and names its boundary. |
| `read_path_limit` | Exceeding the configured path depth yields `AccessPathLimit`, not a falsely absent nested dependency. |

## Verification

Run `cargo test --locked --offline --lib ifds_k021_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
