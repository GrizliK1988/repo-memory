# K025 — Pinned Bluesky PR #5816 acceptance

Status: planned. Roadmap stage: 6.

Dependencies: [K024](024-basic-await.md).

Source: [PR #5816 case study](../../docs/ifds/PR-5816.md) and
[BENCHMARKS section 6](../../docs/ifds/BENCHMARKS.md).

## Scope

- Prepare offline fixtures pinned to the documented base/head hashes, selecting
  only `contentLangs` in `loggedOutFetch`. Verify content hashes/diff application
  and preserve upstream attribution/license when copying source.
- Keep early-stage adaptations visibly separate from exact-source fixtures.
  Declare storage/string/projection/fetch models and all remaining boundaries.
- Promote a reviewed oracle for both unchanged first-request language consumers,
  the changed upstream sources, and explicitly forbidden fallback/labeler flows.

Excludes executing the application or claiming a proven runtime bug cause.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `pinned_fixture_integrity` | Recorded hashes and diff match fixture bytes; source spans resolve the intended binding, not a same-named variable. |
| `source_replacement` | Reviewed reduced fixture replaces contentLanguages/join with appLanguage/split/index provenance. |
| `retained_language_inputs` | URL lang and Accept-Language inputs remain separate retained consumers with changed sources, not newly inserted uses. |
| `getter_vs_new_helper` | Old getter exists but leaves the slice; newly introduced helper operations are actual additions. |
| `no_fallback_or_labeler_flow` | No direct language dependency enters fallback language inputs, and labeler data is not a language origin. |
| `open_network_continuation` | Both external request inputs name modeled boundaries; complete input coverage does not claim lifecycle closure or server-response dependence. |
| `stage_specific_boundaries` | Earlier capability manifests yield the documented unsupported frontiers, not a falsely complete full-source result. |

## Verification

Run `cargo test --locked --offline --lib ifds_k025_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
Also run `cargo test --locked --offline --test ifds bluesky_pr_5816` against pinned
full sources and supplied project metadata. Reduced unit fixtures alone do not
close this task; missing provisioned inputs must fail/flag the gate, not silently skip it.
