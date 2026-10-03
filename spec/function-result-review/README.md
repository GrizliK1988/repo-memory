# Function result report review

Reproducible comparisons for the currently supported synchronous subset: literals, copies and overwrites, branches and write priority, changed guards, early returns, implicit `undefined` at fallthrough, short circuiting, and simple expressions. The final case checks an unsupported call boundary.

Each case directory contains `before.ts`, `after.ts`, a text report, compact JSON, and full JSON evidence. The compact report references the adjacent `full.json`; all references were checked. The text and compact reports were saved after running the command below. In `compact.json`, the evidence path is normalized to the local `full.json`; the text report's absolute temporary path is replaced with a relative one. The reports' semantic fields are preserved.

These reports preserve the output before implementing 001. The [001 implementation review](001-implementation.md) contains regenerated text and current schema information. When rerunning current code, use a fresh evidence sidecar rather than overwriting the archived `full.json`; the CLI rejects an existing sidecar with different content.

Run from a case directory:

```sh
cargo run --locked --offline --manifest-path /home/dima/Develop/repo-memory/Cargo.toml \
  --example ifds_compare -- before.ts after.ts --function result \
  --format text --evidence-out full.json
```

For compact or full JSON, replace `text` with `compact-json` or `full-json`. The full-json mode prints the complete evidence to stdout; text and compact-json write it to `full.json`.

| Case | Cause or control | Reports |
| --- | --- | --- |
| `literal` | Literal replacement | [`report.txt`](reports/literal/report.txt) · [`compact.json`](reports/literal/compact.json) · [`full.json`](reports/literal/full.json) |
| `copy` | Changed source of a saved copy | [`report.txt`](reports/copy/report.txt) · [`compact.json`](reports/copy/compact.json) · [`full.json`](reports/copy/full.json) |
| `priority` | Reordered conditional writes | [`report.txt`](reports/priority/report.txt) · [`compact.json`](reports/priority/compact.json) · [`full.json`](reports/priority/full.json) |
| `guard` | Added condition after early returns | [`report.txt`](reports/guard/report.txt) · [`compact.json`](reports/guard/compact.json) · [`full.json`](reports/guard/full.json) |
| `guard_version` | Overwritten guard value | [`report.txt`](reports/guard_version/report.txt) · [`compact.json`](reports/guard_version/compact.json) · [`full.json`](reports/guard_version/full.json) |
| `early_return` | Added early return | [`report.txt`](reports/early_return/report.txt) · [`compact.json`](reports/early_return/compact.json) · [`full.json`](reports/early_return/full.json) |
| `fallthrough` | Replaced implicit undefined | [`report.txt`](reports/fallthrough/report.txt) · [`compact.json`](reports/fallthrough/compact.json) · [`full.json`](reports/fallthrough/full.json) |
| `short_circuit` | Changed right operand of `&&` | [`report.txt`](reports/short_circuit/report.txt) · [`compact.json`](reports/short_circuit/compact.json) · [`full.json`](reports/short_circuit/full.json) |
| `expression` | Changed computation | [`report.txt`](reports/expression/report.txt) · [`compact.json`](reports/expression/compact.json) · [`full.json`](reports/expression/full.json) |
| `overwritten` | Control: changed write is overwritten | [`report.txt`](reports/overwritten/report.txt) · [`compact.json`](reports/overwritten/compact.json) · [`full.json`](reports/overwritten/full.json) |
| `equal_control` | Control: changed guard with equal result | [`report.txt`](reports/equal_control/report.txt) · [`compact.json`](reports/equal_control/compact.json) · [`full.json`](reports/equal_control/full.json) |
| `unknown` | Control: unsupported call on one branch | [`report.txt`](reports/unknown/report.txt) · [`compact.json`](reports/unknown/compact.json) · [`full.json`](reports/unknown/full.json) |
| `known_expression` | Changed computation with a known numeric source | [`report.txt`](reports/known_expression/report.txt) · [`compact.json`](reports/known_expression/compact.json) · [`full.json`](reports/known_expression/full.json) |

The [semantic expectations and sources](EXAMPLES.md) cover every case. Findings and presentation suggestions are documented separately in [001](001-report-values-and-attribution.md), [002](002-short-circuit-precision.md), and [003](003-compact-readable-text.md).

The [2026-10-03 review of ten complex return-flow examples](complex-batch-2026-10-03/README.md)
includes before/after source, exact CLI output, full evidence, independent concrete
checks, and findings about missing unchanged branches, attribution, and uncertainty
explanations. The archived outputs preserve the working tree used for that review.

[003](003-compact-readable-text.md#agreed-scope) is scoped to text output and its
documentation, preserving JSON structures and versions. Its presentation
requirements and acceptance criteria are implemented and verified; see the
[implementation review](003-implementation.md) and [parity checks](003-verification.json).
[004 — Make the human report more compact](004-evidence-size.md) records the
agreed next text-only improvement: concise function reports by default, detailed
text on request, selective choice tables, key contributing writes, and explicit
control changes. It supersedes the earlier JSON-size proposal and is implemented
and verified; see its [implementation review](004-implementation.md) and
[measurements and parity checks](004-verification.json).
Binding human reports are tracked separately in
[005](005-binding-human-report.md), implemented and verified with an
[implementation review](005-implementation.md),
[parity checks and measurements](005-verification.json), and
[fresh binding reports](binding-reports/README.md).

Schema and reference checks, truth tables, test results, and file sizes are recorded in [verification.json](reports/verification.json). These reports are not implementation-generated golden expectations; expected behavior was derived independently from the source examples.
