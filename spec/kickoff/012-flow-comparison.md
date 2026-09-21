# K012 — Precise before/after deltas

Status: done. Roadmap stage: 1.

Dependencies: [K010](010-full-flow-slices.md), [K011](011-node-alignment.md).

Source: [SPEC section 6](../../docs/ifds/SPEC.md).

## Scope

- Compare independent snapshot graphs using typed relation identities; conditions
  and witnesses are attributes, not reasons to manufacture removed/added edges.
- Produce node, slice-membership, operation, write, flow, condition, value-source,
  and unknown deltas with evidence and deterministic edit groups.
- Keep retained full-graph context separate from claims. Shared derived deltas
  reference all contributing groups once. Do not subtract incomplete edge sets.
  Recompute flow consequences when an aligned operation moves even if its semantic
  fingerprint is unchanged; execution order is not part of operation identity.

Excludes proving runtime value inequality or inventing causes for unresolved deltas.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `initializer_only` | Case 1: operation/source change, retained A-to-R reaches edge, no added/removed write or node. |
| `insert_remove_overwrite` | Case 2 and reverse: B added/removed with its write; A/B-to-R edges swap, but A remains a writer. Include `x=x` preserving transitive origins. |
| `retarget_and_slice` | Case 3: B stops writing x and leaves its slice, without node deletion; reversing the edit re-enters it. |
| `existing_input_switch` | Case 4: p's dependencies leave, q's enter, neither input node is deleted/inserted, and the immediate A-to-R edge stays. |
| `no_downstream_change` | Case 8: overwritten initializer has no return source delta; unrelated/trivia edits have empty query deltas. |
| `presence_truth_table` | Exhaust all nine present/absent/unknown pairs; unknown never proves addition/removal. Identical snapshots retain diagnostics without positive flow deltas. |
| `changed_origin_chain` | Case 10 propagates source changes through B/D/R, not C, while all immediate edges remain. |
| `reordered_operation_flow` | Moving an unchanged write from before to after another write retains its node identity and fingerprint but changes the reviewed reaching writer/consumer flows; emit the applicable flow/source deltas without `NodeAdded`, `NodeRemoved`, or `OperationChanged` for the moved write. |
| `condition_and_group_identity` | Synthetic retained relations with proved changed conditions emit condition deltas only; multiple witnesses/causes do not duplicate a derived finding. |

## Verification

Run `cargo test --locked --offline --lib ifds_k012_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
