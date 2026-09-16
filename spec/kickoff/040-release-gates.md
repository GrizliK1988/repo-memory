# K040 — Coverage registry and release acceptance

Status: planned. Roadmap stage: 11.

Dependencies: [K004](004-benchmark-scoring.md), [K025](025-bluesky-pr-5816.md),
[K039](039-scale-and-measurement.md).

Source: [BENCHMARKS](../../docs/ifds/BENCHMARKS.md) and
[roadmap definition of done](../../docs/ifds/README.md).

## Scope

- Maintain a frozen capability/fixture registry, promoted exact tests, development
  and held-out suite partitions, and explicit supported/unsupported coverage.
- Require positive, negative, before/after, and uncertainty cases per enabled
  syntax/library family; add separate testable task files for uncovered families.
- Assemble reproducible release evidence: semantic regressions, actual precision,
  recall/abstentions, full-flow/ending coverage, and resource measurements.

Excludes calling the analyzer complete for all TypeScript, disguising unsupported
syntax as no-op, or claiming that a 90% benchmark score guarantees every finding.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `family_requires_four_cases` | An enabled family lacking positive, negative, delta, or uncertainty coverage fails registry validation. |
| `promoted_regression_blocks` | Any failed promoted exact fixture blocks release even when aggregate precision passes. |
| `no_quiet_pass` | All-abstention/zero-prediction positive cases cannot produce a passing release. |
| `precision_and_recall_report` | Overall and per-positive-category precision gates apply; all-label and supported-label recall plus abstentions remain visible. |
| `manifest_change_visible` | Removing/reclassifying fixtures or changing model versions requires a new recorded evaluation, not reuse of a passing score. |
| `held_out_integrity` | Duplicate IDs/content across declared development and held-out partitions are detected. |
| `missing_evidence_blocks` | Missing full-flow assertions, PR integration inputs, or real measurement artifacts cannot be marked completed by a unit-only result. |

## Verification

Run `cargo test --locked --offline --lib ifds_k040_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
Then run the actual frozen integration/held-out suite and publish TP/FP/FN,
precision >=90% overall and per supported positive-output category, recall,
coverage, and resource results. The suite runner's correctness and the analyzer's
measured quality are separate acceptance requirements.
