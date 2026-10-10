# K016 loop report examples

These are actual local analyzer outputs, not independently authored acceptance
oracles. The K016 unit tests supply the semantic expectations separately. Each
directory contains before/after TypeScript, the short function report, and its
exact full JSON evidence. An additional binding report is saved for `added_loop`.
No analyzed JavaScript was executed to produce these reports.

Short text compares uniquely corresponding loops in one block and shows changed
conditions, initialization, updates, body operations and operation order with
before/after locations. Unchanged body operations are omitted. Repeated writes
are source edits, not proofs of a changed final return value or an exact iteration
count. `--verbose` retains the separate before/after loop descriptions.

| Case | Expected semantic result | Output |
| --- | --- | --- |
| Added one-shot loop | `again` selects `0 -> 1` (Different); `!again` retains `0` (Equal). No repeated body execution. | [Function report](added_loop/report.txt), [binding report](added_loop/binding-report.txt) |
| Changed write in a one-shot loop | `again` selects `1 -> 2` (Different); `!again` retains `0` (Equal). The body sets `again = false`, so it executes at most once. | [Function report](changed_return/report.txt) |
| Conditional write added inside a loop | Only `again && boost` changes the return from `1` to `2` (Different). `!again` retains `0`, and `again && !boost` retains `1` (Equal). Attribution links the added guard to the reaching write and return. | [Function report](conditional_return/report.txt) |
| Changed accumulation | The loop exposes `step`, carried `remaining`/`x`, and possible repeated writes. Numeric path feasibility, accumulated results and completion remain unresolved. | [Function report](accumulation/report.txt) |
| While replaced with forEach | The established while structure remains visible for the before snapshot. Callback execution and normal completion in the after snapshot are unresolved. | [Function report](unsupported_iteration/report.txt) |

`again` in these reports is an input-region condition derived from the captured
first test, even though the body later writes `again = false`. The local at-exit
value does not replace the input-region condition. `loop carried` also applies
to values reaching condition reevaluation; it does not by itself mean the body
executes twice.

Source alignment and edit attribution are separate from result proofs. Adding
a new control construct can leave attribution partial while the literal result
comparison is complete. No exact iteration count is inferred from fixed-point
convergence or from an abstract continuing cycle.

To regenerate one function report from the repository root:

```sh
cargo run --locked --offline --example ifds_compare -- \
  spec/kickoff/016-loop-reports/added_loop/before.ts \
  spec/kickoff/016-loop-reports/added_loop/after.ts \
  --function f --format text \
  --evidence-out spec/kickoff/016-loop-reports/added_loop/full.json
```

Replace `--function f` with `x` for the binding target and choose a separate
evidence path. Current function analysis/full/compact schemas are 5/6/6 and binding
schemas are 4/3. Archived reports retain the schemas used when they were saved.
