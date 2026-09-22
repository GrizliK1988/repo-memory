# K011 — Cross-revision identity alignment

Status: done. Roadmap stage: 1.

Dependencies: [K002](002-snapshots-and-queries.md), [K005](005-typescript-bindings.md).

Source: [SPEC sections 3 and 6.1](../../docs/ifds/SPEC.md).

## Scope

- Align declarations and operations using explicit counterparts, file/rename
  mapping, lexical roles, structure, and local edit evidence.
- A local declaration may correspond to a parameter of the same containing
  function, and vice versa. Accept an explicit counterpart when the file and
  containing lexical scope correspond. Infer this role change only when no
  same-role match exists and the name, scope, and file identify one candidate.
  Keep the binding's logical identity, but represent its initializer write and
  parameter-entry source separately; their value origins are not equivalent.
- Separate logical identity from expression fingerprints and source locations.
  A new RHS must not manufacture a new binding/write identity. Preserve the
  identity and fingerprint of a uniquely aligned operation that only moves, while
  retaining its changed execution order for flow comparison.
- Preserve ambiguity as a diagnostic with candidate mappings; do not shift all
  repeated-call identities by occurrence index after an insertion.

Excludes probabilistic matching or silently selecting among credible counterparts.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `initializer_identity` | `let x=1` to `let x=2` retains declaration/write logical IDs while the semantic fingerprint changes. |
| `trivia_and_lines` | Whitespace/comments/line shifts preserve mappings and fingerprints; locations update. |
| `rename_mapping` | A validated file rename maps the same declaration; inconsistent rename metadata does not. |
| `retarget_operation` | A uniquely corresponding `x=2` to `y=2` assignment retains its operation ID but changes target fingerprint. |
| `moved_operation_identity` | Moving an otherwise unchanged assignment across another write retains its logical ID and semantic fingerprint, updates its location/order, and does not classify it as added, removed, or changed merely because it moved. |
| `repeated_call_ambiguity` | An indistinguishable extra call leaves competing correspondences unresolved; no forced index-based match or deletion claim. |
| `explicit_counterpart` | A valid counterpart disambiguates; a stale or incompatible counterpart is rejected. |
| `one_sided_binding` | A confirmed inserted/deleted selected binding is represented one-sidedly without aliasing a same-named neighbor. |
| `local_parameter_transition` | `function f() { let x=1; return x }` to `function f(x) { return x }` aligns `x` with and without an explicit counterpart. The local initializer write is one-sided and the parameter input is visible in the after flow. The reverse transition also aligns. A different containing function remains incompatible. |

## Verification

Run `cargo test --locked --offline --lib ifds_k011_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
