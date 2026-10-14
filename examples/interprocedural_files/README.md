# Result change through functions in separate files

Each snapshot contains four modules:

- `main.ts`: the selected `buildMessage` calls `makeGreeting` and `addPunctuation`.
- `greeting.ts`: `makeGreeting` imports and calls `greetingPrefix`.
- `prefix.ts`: `greetingPrefix` returns the greeting prefix.
- `punctuation.ts`: `addPunctuation` adds `"!"`.

Only `prefix.ts` changes: `"Hello, "` becomes `"Welcome, "`.

From the repository root:

```sh
cargo run --locked --offline --example ifds_compare -- \
  examples/interprocedural_files/before \
  examples/interprocedural_files/after \
  --function buildMessage --metadata \
  --evidence-out examples/interprocedural_files/report.json
```

Directory mode recursively loads `.ts` and `.tsx` files, skips `node_modules`
and `.git`, and supplies a virtual tsconfig with ES2022 and bundler resolution.
It requires `--function` and `--metadata`; `--entry-file` defaults to `main.ts`.
Custom tsconfigs and package manifests are not loaded by this example runner.

Captured output:

```text
Function result: buildMessage
Edit attribution uncertain
Changed computation under always:
  (("Hello, " + "Ada") + "!") -> (("Welcome, " + "Ada") + "!") (Changed)
  Reaches return addPunctuation(greeting)
Before contributing writes: greeting.ts:4, main.ts:5, prefix.ts:2, punctuation.ts:2
After contributing writes: greeting.ts:4, main.ts:5, prefix.ts:2, punctuation.ts:2
Coverage: result comparison Complete; source alignment Complete; data flow Complete
Evidence: examples/interprocedural_files/report.json (ifds-function-2bab6d19ae6233e5)
```

The analyzer follows imports and nested direct calls to report the changed
computation reaching the outer return. All three coverage dimensions are complete,
but edit attribution remains uncertain and the assessment is `Changed`, rather
than the stronger `Different`. Full evidence is saved in [report.json](report.json).

Execute these authored example modules separately to confirm their concrete values:

```sh
node examples/interprocedural_files/run.cjs
```

```text
before: "Hello, Ada!"
after: "Welcome, Ada!"
```

The IFDS comparison itself analyzes the modules statically.
