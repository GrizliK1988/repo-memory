# K001 — Module contracts and report model

Status: done. Roadmap stage: 0.

Dependencies: none.

Source: [SPEC sections 2–4 and 7](../../docs/ifds/SPEC.md).

## Scope

- Introduce the separate public `ifds` module and language-independent IDs, IR
  operation/edge types, finite fact types, query/result contracts, and solver interface.
- Model all delta/evidence kinds, source spans, entry assumptions, directional
  coverage, path endings, diagnostics, model versions, and statistics.
- Keep snapshot-local IDs distinct from cross-revision logical IDs. Provide
  deterministic serialization; do not couple core types to Tree-sitter or JSX.

Excludes parsing, solving, and reusing `ChildChange` as the flow report.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `round_trip_report` | Serialize/deserialize a report containing every delta, evidence, and ending variant; all semantic fields and UTF-8 spans survive. |
| `deterministic_serialization` | Insert the same nodes/facts in opposite orders; canonical serialized report bytes match. |
| `snapshot_ids_do_not_alias` | Equal local numeric IDs in two snapshots stay distinct; only an explicit logical mapping connects them. |
| `ir_without_language_adapter` | Construct and validate a tiny Entry/Write/Read/Exit IR using core types alone. |
| `coverage_is_not_lifecycle` | Represent a complete modeled request-input slice with an open boundary; lifecycle closure remains false. |
| `reject_invalid_schema` | Unsupported schema versions and invalid byte/line spans fail with structured errors, not panics. |

## Verification

Run `cargo test --locked --offline --lib ifds_k001_`, following the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
