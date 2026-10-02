# 005 implementation review

[005](005-binding-human-report.md) is implemented for binding human text.
A shared producing-expression or selection change is described once and linked
to its affected uses, including computations and exact operand roles. Positions
are explicit for both snapshots; moved uses retain both positions. Grouping is
presentation-only and never changes structured observation groups or findings.

An unchanged transparent copy may be omitted only with an exhaustive,
unconditional single origin, complete comparison, and an established consumer
on both sides. A terminal copy remains visible as a use. Edited selected writes
that are overwritten before any use are hidden in short text, while detailed
text and full evidence retain them. A scoped empty report does not claim equal
results or absence of all source edits.

Selection explanations retain necessary fallbacks, actual reaching conditions,
and both overwrite orders when precedence changes. A writer enabled under `a`
can still be selected only under `a && !b`; that distinction remains visible.
Nested guard changes are factored only through a recorded common controller;
independent changes remain separate. Identical guard text with different value
versions gets distinct guard IDs. Missing positions or compact selection rules
are marked unresolved.

Changing an expression at a use, or adding/removing an actual consumer, remains
visible even when compact findings alone do not list that observation change.
These presentation facts use existing node-presence, source, observation, and
full-delta evidence. Analysis/comparison semantics and JSON findings are unchanged.
No whole-function equality, arithmetic evaluation, or certain edit attribution
is inferred from binding flow.

## Compatibility and CLI

`VariableSourceReport::render_text(&full)` renders short text;
`render_verbose_text(&full)` renders detailed text. They verify the compact/full
identity and references before rendering and expose the full evidence's limits,
coverage, and boundaries. The former no-argument rendering is retained as
`render_legacy_text()` and is used exclusively to preserve `human_summary` during
API report assembly and CLI sidecar writes. This renderer-API migration is
intentional and documented; the existing full analysis APIs and JSON schemas
are preserved.

Binding text is short by default. `--verbose` now works for either target with
text format, while both JSON formats reject it before source/evidence I/O.
The rejection wording now states the shared text-format restriction. Valid
function-target output from 004 remains byte-identical. Both text modes point
at the exact full evidence of their analysis. Selecting verbosity for a fixed
structured report does not change its content, ID, or evidence references.

The existing evidence-path selection and overwrite protection remain. An
independent CLI analysis can still produce a different ID because binding
`stats.elapsed_ms` participates in the hash; this existing limitation was not
changed. Stored `human_summary` wording and path handling remain compatible.

## Fresh measurements and parity

[005-verification.json](005-verification.json) records exact source pairs,
queries, line/byte measurements, texts, IDs, and parity results for 16 synthetic
binding cases and 13 function controls. The baseline is commit
`374919a6429de2d3741335cb5ce21c4b3fcf4011`. Measurements normalize the evidence path
to `full.json` and include the final newline. These are readability examples,
not a project performance estimate. The old archives remain intact.

| Case | Legacy bytes | Short bytes | Detailed bytes | Legacy lines | Short lines | Detailed lines |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `copy_chain` | 643 | 355 | 1966 | 13 | 6 | 37 |
| `copy_then_overwrite` | 151 | 209 | 1298 | 2 | 3 | 25 |
| `early_return` | 461 | 588 | 2487 | 11 | 12 | 49 |
| `full_downstream_chain` | 739 | 367 | 2358 | 16 | 6 | 42 |
| `guard` | 366 | 370 | 1837 | 7 | 7 | 30 |
| `guard_versions` | 423 | 829 | 2164 | 7 | 12 | 33 |
| `independent_expressions` | 473 | 526 | 1491 | 11 | 10 | 30 |
| `moved_positions` | 366 | 384 | 1865 | 7 | 7 | 30 |
| `nested_guard` | 489 | 558 | 2741 | 9 | 9 | 41 |
| `new_copy` | 189 | 261 | 801 | 2 | 6 | 17 |
| `operand_expression` | 194 | 297 | 917 | 2 | 6 | 16 |
| `overwrite` | 295 | 380 | 1182 | 5 | 7 | 23 |
| `overwritten_edit` | 264 | 203 | 1320 | 5 | 3 | 26 |
| `priority` | 250 | 586 | 2657 | 3 | 9 | 37 |
| `unchanged` | 189 | 203 | 865 | 2 | 3 | 16 |
| `unknown` | 263 | 510 | 1341 | 4 | 7 | 21 |

The main chains lose repeated explanations: `full_downstream_chain` falls from
16 to 6 lines and 739 to 367 bytes; `copy_chain` falls from 13 to 6 lines and 643
to 355 bytes. Overwritten edits become a scoped empty-use-change report.
Across all cases, short output is 6,626 bytes/113 lines versus detailed output's
27,290 bytes/473 lines. The previous default was 5,755 bytes/106 lines: aggregate
bytes increase by 15.1% because positions, both priority directions, reasons,
boundaries, and previously unreported actual consumer changes are now explicit.
There is no fixed cap that suppresses these facts.

Full and compact binding JSON, including legacy `human_summary`, are equivalent
to the baseline after normalizing only existing elapsed-time differences,
derived IDs, and evidence paths. Current short, detailed, and compact modes have
the same normalized sidecar content. Fixed-report Rust checks verify unchanged
canonical bytes before/after rendering, stable text, and serialization round trips.
All 514 compact references and all source positions were checked against their
exact sidecars and source snapshots. Every text evidence link matches its own
sidecar's actual ID. All 13 function controls match the baseline byte-for-byte
in both text modes and both JSON formats at a shared evidence path.

Four unsupported verbosity/JSON combinations fail before source I/O, produce no
stdout/sidecar, and report the text-format restriction. An existing conflicting
sidecar survives unchanged. Focused regression tests independently cover source
factoring, computations versus copies, terminal/new copies, hidden overwritten
edits, moved positions, reaching versus enabling conditions, nested and independent
guard changes, guard versions, distinct equal-RHS writers, expression edits at
operand uses, unknowns, analysis limits, presentation omissions, and bad references.

[Fresh binding reports](binding-reports/README.md) retain exact full evidence
for each recorded CLI execution. These reports are verification observations,
not generated semantic golden expectations.

## Reproduction

Build a baseline executable in a fresh directory, then run the current verifier:

```sh
review005_baseline_dir="$(mktemp -d /tmp/review-005-baseline.XXXXXX)"
git archive 374919a6429de2d3741335cb5ce21c4b3fcf4011 | tar -x -C "$review005_baseline_dir"
cargo build --locked --offline --manifest-path "$review005_baseline_dir/Cargo.toml" --example ifds_compare
cargo build --locked --offline --example ifds_compare
python3 spec/function-result-review/verify_005.py \
  --baseline "$review005_baseline_dir/target/debug/examples/ifds_compare" \
  --output /tmp/review-005-verification.json
```

Add `--reports-dir <new-empty-directory>` to retain fresh sidecars and texts.
The verifier refuses to overwrite a nonempty report archive. Test/lint results
are recorded separately in the checked-in verification artifact.

Validation passed: `cargo test --locked --offline --quiet` (237 unit, 9 integration,
and 2 documentation tests), `cargo fmt --all -- --check`, and
`cargo clippy --locked --offline --all-targets -- -D warnings`.
