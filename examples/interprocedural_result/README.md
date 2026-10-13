# Result change through a chain of functions

[before.ts](before.ts) and [after.ts](after.ts) contain four functions. Only
`greetingPrefix` changes: `"Hello, "` becomes `"Welcome, "`. `makeGreeting`,
`addPunctuation`, and the selected comparison target `buildMessage` are identical
in both versions.

The result dependency is `greetingPrefix()` → `makeGreeting("Ada")` →
`addPunctuation(greeting)` → the result of `buildMessage()`.

From the repository root, run the comparison with TypeScript metadata:

```sh
cargo run --locked --offline --example ifds_compare -- \
  examples/interprocedural_result/before.ts \
  examples/interprocedural_result/after.ts \
  --function buildMessage --metadata \
  --evidence-out examples/interprocedural_result/report.json
```

`--metadata` uses the installed Node/TypeScript provider and a virtual tsconfig
for these two single-file snapshots. See the
[provider documentation](../../docs/ifds/PROJECT_METADATA.md) for dependency setup.

Captured output:

```text
Function result: buildMessage
Edit attribution uncertain
Changed computation under always:
  (("Hello, " + "Ada") + "!") -> (("Welcome, " + "Ada") + "!") (Changed)
  Reaches return addPunctuation(greeting)
Before contributing writes: snippet.ts:2, snippet.ts:6, snippet.ts:10, snippet.ts:14
After contributing writes: snippet.ts:2, snippet.ts:6, snippet.ts:10, snippet.ts:14
Coverage: result comparison Complete; source alignment Partial; data flow Complete
Evidence: examples/interprocedural_result/report.json (ifds-function-2a6649ab24ae4a56)
```

Full dependencies and call contexts are saved in [report.json](report.json).
The analysis reports `Changed`: the computation reaching the final return has
changed. Source-node alignment is partial, so precise edit attribution remains
uncertain; this report does not claim the stronger `Different` assessment.

Separate execution of the example confirms the concrete values:

```sh
node examples/interprocedural_result/run.cjs
```

```text
before: "Hello, Ada!"
after: "Welcome, Ada!"
```

This run executes only the example code in separate Node contexts. The IFDS
analysis itself builds dependencies statically.
