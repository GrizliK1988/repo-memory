# K003 — Fixture harness and independent oracles

Status: planned. Roadmap stage: 0.

Dependencies: [K001](001-contracts.md).

Source: [BENCHMARKS sections 1 and 5](../../docs/ifds/BENCHMARKS.md).

## Scope

- Define fixture manifests, before/after sources, logical labels, reviewed expected
  required/forbidden/retained relations, coverage, endings, and witness assertions.
- Build reusable unit-test helpers plus an explicit `tests/ifds.rs` integration
  entry point loading cases under `tests/ifds/`; nested directories alone are not
  Cargo test targets.
- Add an independent tiny reference evaluator for finite hand-authored IR. It must
  not reuse production transfer/comparison logic as its correctness oracle.

Excludes generating expected outputs from the analyzer or fetching fixtures online.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `manifest_validation` | Missing selectors, model versions, or expected files produce a fixture error naming the missing field/file. |
| `required_forbidden_retained` | Synthetic reports missing a required edge, adding a forbidden edge, or misclassifying a retained edge each fail independently. |
| `full_chain_required` | A report with only the first consumer fails when the oracle requires the later consumer and ending. |
| `logical_label_normalization` | Different runtime IDs mapped to reviewed labels compare equal; swapped source/consumer labels do not. |
| `tiny_reference_evaluator` | Hand-authored overwrite/copy graphs yield reviewed last-write/origin sets without production solver calls. |
| `distributivity_harness` | Exhaustive tiny-domain checks accept a known distributive rule and reject a deliberately conjunctive two-fact rule. |
| `bounded_oracle_is_honest` | Exhaustive finite inputs permit an exact oracle; an insufficient execution bound reports incompleteness, never absence proof. |

## Verification

Run `cargo test --locked --offline --lib ifds_k003_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
Confirm `cargo test --locked --offline --test ifds -- --list` discovers the initial
hand-authored fixtures; empty discovery fails this task.
