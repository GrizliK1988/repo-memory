# K039 — Demand slicing, parallelism, and measurement

Status: planned. Roadmap stage: 11.

Dependencies: [K038](038-caching.md), [K035](035-generators-and-iterators.md),
[K036](036-framework-models.md), [K037](037-module-execution.md).

Source: [SPEC section 8](../../docs/ifds/SPEC.md) and
[benchmark measurement rules](../../docs/ifds/BENCHMARKS.md).

## Scope

- Optimize with demand-driven expansion and independent snapshot parallelism while
  preserving complete selected upstream/downstream relations and evidence.
- Bound work/output, reuse K009's limits, and collect cold/warm duration, peak
  process-tree memory, repository size, cache, and metadata-provider costs.
- Provide deterministic measurement aggregation; calibrate the documented proposed
  medium-repository targets rather than asserting them from synthetic tests.

Excludes silently truncating to edited files, first consumers, or fixed loop depths.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `demand_reference_equivalence` | Demand-sliced and unsliced small fixtures have identical required relations, source changes, and endings. |
| `parallel_determinism` | Alternate worker completion orders produce identical canonical graphs/deltas/group IDs with no cross-snapshot facts. |
| `downstream_not_pruned` | An unchanged remote-file consumer and an escaping copy survive optimization. |
| `bounded_output` | Witness-only and graph truncation retain their distinct completeness/count semantics under optimized execution. |
| `fake_measurements` | Known synthetic duration samples produce correct p50/p95; provider time is included and provisioning time excluded. |
| `memory_and_budget_records` | Injected process-tree memory/counter limits produce explicit partial reports and correct recorded peak/limit reasons. |
| `warm_cold_equality` | Performance modes do not change semantic claims even when timing/cache statistics differ. |

## Verification

Run `cargo test --locked --offline --lib ifds_k039_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
Additionally measure a pinned medium-sized repository on recorded hardware. Report
cold/warm p95 and peak memory against the proposed 120s/15s/4GiB targets; record
misses honestly. Mock-clock unit tests do not establish real performance acceptance.
