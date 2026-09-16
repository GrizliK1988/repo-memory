# K026 — Heap places, aliases, and mutation

Status: planned. Roadmap stage: 7.

Dependencies: [K021](021-read-projections.md), [K023](023-exceptions.md).

Source: [SPEC heap-transfer constraints](../../docs/ifds/SPEC.md) and
[roadmap stage 7](../../docs/ifds/README.md).

## Scope

- Introduce allocation-site heap places and bounded property/index paths for
  object/array reads and writes, mutation through arguments, and audited mutators.
- Apply strong updates only with proven unique targets; ambiguous aliases use
  weak updates and explicit uncertainty where required.
- Freeze alias information for one IFDS solve; changed alias assumptions invalidate
  dependent facts/summaries. Distinguish a binding reassignment from a property write.

Excludes treating arbitrary library mutators as modeled merely by method name.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `alias_property_write` | `b=a; b.field=x` may update reads of `a.field`; reassigning b alone does not rewrite a's binding. |
| `strong_update` | A proved unique property target kills its previous property writer without killing sibling fields. |
| `weak_update` | Multiple possible target allocations retain old and new possible origins; no unsupported strong kill. |
| `argument_mutation` | A resolved callee's object-field write reaches the caller's aliased field, unlike scalar parameter reassignment. |
| `array_mutator` | An audited array update has its declared element effects; an unknown mutator opens an effect boundary. |
| `fixed_alias_distributivity` | With alias metadata fixed, each heap transfer distributes over finite fact-set union. |
| `overflow_and_invalidation` | Path bounds produce `AccessPathLimit`; changed alias metadata invalidates affected results rather than reusing stale facts. |

## Verification

Run `cargo test --locked --offline --lib ifds_k026_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
