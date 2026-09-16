# K038 — Summary caching and invalidation

Status: planned. Roadmap stage: 11.

Dependencies: [K025](025-bluesky-pr-5816.md), [K026](026-heap-and-aliases.md),
[K033](033-identity-refinement.md).

Source: [SPEC section 8](../../docs/ifds/SPEC.md).

## Scope

- Cache metadata/analysis summaries with content-addressed keys covering snapshot
  bodies, dependencies, configuration, analysis/model/provider versions, alias
  assumptions, and applicable entry context.
- Invalidate changed callees and transitive consumers; never reuse an incomplete
  result as a complete solve or alias records across revisions accidentally.
- Preserve deterministic canonical results with cache hits/misses and report metrics.

Excludes claiming cache correctness from improved runtime alone.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `cached_equals_cold` | Supported fixture graphs/deltas/endings are identical with cold, warm, and disabled caches. |
| `callee_invalidates_callers` | A callee source edit invalidates affected callers and changes unchanged consumer provenance. |
| `key_dimensions` | Mutating each config/model/provider/alias/context key component causes the required cache miss. |
| `unrelated_edit_reuse` | A proven independent edit retains unaffected reusable summaries without reusing stale changed ones. |
| `partial_not_complete` | A budget-truncated cache entry cannot satisfy a later complete-analysis request. |
| `snapshot_isolation` | Identical local node numbers in different snapshots cannot retrieve the wrong facts or source spans. |
| `corrupt_entry` | An incompatible/corrupt cache entry is rejected or recomputed with a diagnostic, never silently accepted as evidence. |

## Verification

Run `cargo test --locked --offline --lib ifds_k038_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
