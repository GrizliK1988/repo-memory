# K004 — Late benchmark scoring and acceptance gates

Status: deferred. Roadmap stage: 11.

Dependencies: [K003](003-test-harness.md), [K025](025-bluesky-pr-5816.md),
[K039](039-scale-and-measurement.md).

Source: [BENCHMARKS sections 2–3](../../docs/ifds/BENCHMARKS.md).

## Scope

- Implement this task only after the analyzer, promoted semantic fixtures, pinned
  real-repository cases, and an independently reviewed integration/held-out sample
  are available. Earlier stages rely on exact unit and integration assertions, not
  precision estimates over their small synthetic fixture sets.
- Canonicalize claims by the specified query, delta, relation, source, consumer,
  input projection, changed condition/origin, and before/after presence tuple.
- Compute TP/FP/FN, precision, recall, category results, abstentions, and coverage;
  separate structural/context assertions from scored flow claims.
- Implement exact-fixture gates and the reviewed-suite >=90% precision gates.
  Freeze model/capability manifests; never hide missed cases by reclassification.

Excludes claiming that a scoring implementation proves actual analyzer precision.
Also excludes using early synthetic fixtures to publish or enforce a representative
precision percentage before the frozen reviewed suite exists.

Until this task begins, additions/removals/condition changes/source changes,
forbidden flows, full context, endings, and uncertainty are ordinary exact test
assertions owned by their implementation tasks and the K003 fixture harness.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `known_counts` | Expected `{a,b,c}`, predicted `{a,b,d}` yields TP=2, FP=1, FN=1, precision=recall=2/3. |
| `deduplicate_witnesses` | Three witnesses for one canonical claim count once; direct and transitive relation kinds remain separate. |
| `abstention_is_miss` | An unresolved expected positive contributes FN, not TP; a wrong modeled or “may” claim contributes FP. |
| `empty_predictions` | Positive expected set with no predictions gives precision N/A, recall 0, and a failing fixture; empty negative cases add no fictitious TP. |
| `category_gate` | Aggregate precision >=90% with one positive-output category below 90% fails; exactly 90% passes the precision threshold. |
| `context_does_not_inflate` | Unchanged graph nodes, endings, and function summaries do not affect TP; a missing required chain still fails structural acceptance. |
| `frozen_manifest` | Silently dropping a fixture or changing its supported category invalidates the evaluation. |

## Verification

Run `cargo test --locked --offline --lib ifds_k004_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
