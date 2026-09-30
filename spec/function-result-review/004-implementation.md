# 004 implementation review

[004](004-evidence-size.md) is implemented for function-result human text.
The default report removes redundant tables for simple transitions, merges their
context into the effect condition, and shows key contributing writes once per
finding and snapshot. `--verbose` retains the 003 detailed text presentation.

Conditional selection tables remain for choices with conditions beyond the
finding's common context or multiple directional effects. This keeps the guard,
write-priority, and grouped-swap selection rules visible. Short-circuit operand
selection keeps its established truthiness wording.

Key write selection traverses the full recorded return dependencies. It omits a
transparent copy only when a resolved single-source chain reaches a contributing
write with a source position. Computations and multiple sources are retained;
missing source positions or unresolved chains do not justify dropping a located
outer write. Positions are deduplicated per finding and snapshot. The source list
describes contributors and does not establish edit attribution.

Default text explicitly reports control changes for changed and unknown findings,
as well as equal-result control changes. Multiple unchanged values in one control
finding share a scoped equality summary; their individual values and regions
remain in detailed text and evidence. Unknown reasons, coverage, query scope,
entry/domain/presence notices, attribution uncertainty, limits, and evidence links
retain the 003 safeguards.

The API's `render_text(&full)` uses concise rendering;
`render_verbose_text(&full)` uses detailed rendering. The example CLI accepts
`--verbose` only with a function target and text format, rejecting other
combinations before reading source files or writing evidence.

## Measurements and parity

Fresh reports use the same 13 archived source pairs, query defaults, and baseline
commit `c012ef8761a78bfaad7e699034d5555823b1c457`. Text measurements normalize only the temporary
evidence path to `full.json` and include the final newline. These are synthetic
examples, not a project performance estimate. Archived report files were untouched.

| Case | Previous / verbose bytes | Short bytes | Previous / verbose lines | Short lines |
| --- | ---: | ---: | ---: | ---: |
| `copy` | 472 | 299 | 10 | 7 |
| `early_return` | 442 | 492 | 9 | 10 |
| `equal_control` | 256 | 256 | 4 | 4 |
| `expression` | 197 | 197 | 4 | 4 |
| `fallthrough` | 302 | 209 | 9 | 5 |
| `guard` | 713 | 763 | 11 | 12 |
| `guard_version` | 320 | 371 | 9 | 10 |
| `known_expression` | 423 | 270 | 9 | 6 |
| `literal` | 244 | 175 | 7 | 4 |
| `overwritten` | 230 | 230 | 4 | 4 |
| `priority` | 703 | 558 | 11 | 12 |
| `short_circuit` | 188 | 188 | 4 | 4 |
| `unknown` | 403 | 318 | 10 | 6 |
| Total | 4893 | 4326 | 101 | 88 |

Aggregate text bytes fell by 11.6%, and lines by 12.9%. `copy` fell from 472 to
299 bytes; `known_expression` from 423 to 270; `priority` from 703 to 558.
Some control-change reports grew because they now explicitly report the changed
control alongside its effects. There is no fixed size limit hiding those facts.

[004-verification.json](004-verification.json) records the complete short and
detailed texts, sizes, report IDs, hashes, and parity results. For every function
case, full JSON stdout and compact JSON are byte-identical to the baseline at the
same fresh sidecar path. Full stdout matches the sidecar except for its final
newline. Detailed text is byte-identical to previous default text. Repeated short
and detailed runs are deterministic, and default text matches explicit text mode.
Full report schema 2, analysis schema 4, and compact schema 2 remain unchanged.

Three binding fixtures were exercised in all formats. Binding content is
unchanged after normalizing existing runtime-dependent `stats.elapsed_ms`, derived
report IDs, and temporary sidecar paths. This limitation predates 004: binding IDs
include runtime stats, so independent CLI runs need not be byte-identical. The
binding renderer and identity implementation were not changed; follow-up work is
tracked separately in [005](005-binding-human-report.md).

All five unsupported combinations of `--verbose` with binding targets or JSON
formats fail explicitly before source I/O and produce no sidecar or stdout.

## Short examples

### Copy

```text
Function result: result
Edit attribution uncertain
Effect: always: saved (1) -> saved (2) (Different)
Before contributing writes: snippet.ts:2
After contributing writes: snippet.ts:2
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-c1143f1c678a1ce3)
```

The writes at line 2 supply the returned values. The intermediate copy at line 3
is available in detailed text, while the later overwrite at line 4 is never
reported as a contributor.

### Fallthrough

```text
Function result: result
Edit attribution uncertain
Effect: !flag: undefined -> 2 (Different)
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-3ec23b63948b2cba)
```

### Conditional swap

```text
Function result: result
Edit attribution uncertain
Control: g (resolved: flag) -> g (resolved: !flag)
Return choice | Before | After
"no" | !flag | flag
"yes" | flag | !flag
Effect: !flag: "no" -> "yes" (Different)
Effect: flag: "yes" -> "no" (Different)
Coverage: result comparison Complete; source alignment Partial
Evidence: full.json (ifds-function-a598f5826cd9b5c6)
```

## Reproduction

Build the current example and a baseline executable from the recorded commit:

```sh
review004_baseline_dir="$(mktemp -d /tmp/review-004-baseline.XXXXXX)"
git archive c012ef8761a78bfaad7e699034d5555823b1c457 | tar -x -C "$review004_baseline_dir"
cargo build --locked --offline --manifest-path "$review004_baseline_dir/Cargo.toml" --example ifds_compare
cargo build --locked --offline --example ifds_compare
python3 spec/function-result-review/verify_004.py \
  --baseline "$review004_baseline_dir/target/debug/examples/ifds_compare" \
  --baseline-commit c012ef8761a78bfaad7e699034d5555823b1c457 \
  --output /tmp/review-004-verification.json
```

The verification script uses fresh temporary sidecars and performs CLI parity and
invalid-option checks. Compare its case measurements with the checked-in artifact;
repository test/lint results are recorded separately in that artifact.

Validation passed: `cargo test --locked --offline` (221 unit, 9 integration, and
2 documentation tests), `cargo fmt --all -- --check`, and
`cargo clippy --locked --offline --all-targets -- -D warnings`. Renderer regressions
cover simple/scoped replacements, resolved copies versus computations and multiple
origins, conditional swaps, source deduplication, and scoped equal-choice summaries.
Existing function-result integration tests continue to exercise unknowns, entry
scope, presence/domain boundaries, round trips, and reference validation.
