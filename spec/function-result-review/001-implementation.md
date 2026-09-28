# 001 implementation review

The text below was regenerated with the comparison CLI after implementing [001](001-report-values-and-attribution.md). Original observations under [reports](reports/README.md) remain archived. These excerpts illustrate the implementation; source semantics and assertions in the reporting tests remain the acceptance oracles.

The CLI uses `snippet.ts` as the logical path for both local snapshots. `Before` locations refer to each case's `before.ts`, and `After` locations to `after.ts`.

## Schema changes

Compact schema 2 stores each result's expression, optional typed primitive value, identity, return references, and contributing dependency paths/positions. Each grouped effect retains its region and directional results. Full report schema 2 and analysis schema 4 add dependency source positions so those links can be validated. Existing result assessments and binding-report schemas are preserved. Readers reject older function-report versions; archived reports are not migrated.

## Regenerated text

### copy

Sources: [before](reports/copy/before.ts) and [after](reports/copy/after.ts).

```text
Function result: result
Return choice | Before | After
saved (1) | always | never
saved (2) | never | always
Effect: always: saved (1) -> saved (2) (Different)
Before return saved at snippet.ts:5; contributing writes: snippet.ts:3, snippet.ts:2
After return saved at snippet.ts:5; contributing writes: snippet.ts:3, snippet.ts:2
Evidence: full.json (ifds-function-c1143f1c678a1ce3)
```

### priority

Sources: [before](reports/priority/before.ts) and [after](reports/priority/after.ts).

```text
Function result: result
Return choice | Before | After
x (1) | a && !b | a
x (2) | b | !a && b
Effect: a && b: x (2) -> x (1) (Different)
Before return x at snippet.ts:5; contributing writes: snippet.ts:4
After return x at snippet.ts:5; contributing writes: snippet.ts:4
Unchanged choices: x (0)
Equal under !a && !b: x (0)
Equal under !a && b: x (2)
Equal under a && !b: x (1)
Evidence: full.json (ifds-function-bb514a929c428eee)
```

### guard_version

Sources: [before](reports/guard_version/before.ts) and [after](reports/guard_version/after.ts).

```text
Function result: result
Return choice | Before | After
"no" | !flag | flag
"yes" | flag | !flag
Effect: !flag: "no" -> "yes" (Different)
Effect: flag: "yes" -> "no" (Different)
Evidence: full.json (ifds-function-a598f5826cd9b5c6)
```

### known_expression

Sources: [before](reports/known_expression/before.ts) and [after](reports/known_expression/after.ts).

```text
Function result: result
Return choice | Before | After
input + 1 | always | never
input + 2 | never | always
Effect: always: input + 1 -> input + 2 (Changed)
Before return input + 1 at snippet.ts:3; contributing writes: snippet.ts:2
After return input + 2 at snippet.ts:3; contributing writes: snippet.ts:2
Evidence: full.json (ifds-function-2c8828ab71e2dbbd)
```

## Reproduction and verification

From the repository root, choose a fresh sidecar path and run:

```sh
cargo run --locked --offline --example ifds_compare -- \
  spec/function-result-review/reports/copy/before.ts \
  spec/function-result-review/reports/copy/after.ts \
  --function result --format text --evidence-out /tmp/copy-001-full.json
```

Use `compact-json` or `full-json` for the other formats. Select the corresponding archived sources to reproduce another case. A sidecar path already containing a different report is rejected.

All 13 archived source pairs were rerun in all three formats. Every compact comparison/observation reference and contributing dependency path, node identity, and source position was checked against the generated full sidecar; full JSON stdout matched the sidecar. The values, regions, and conservative computation boundaries are covered by the function-result unit and integration tests.

Validation passed with `cargo test --locked --offline`, `cargo fmt --all -- --check`, and `cargo clippy --locked --offline --all-targets -- -D warnings`.
