# Status after fixing condition attribution

Date: 2026-10-03. This supplements the [original ten-example report](README.md).
Archived stdout, stderr, and evidence describe the previous engine and remain
unchanged. The code published with this follow-up was checked.

All ten source pairs from `cases.json` were rerun through
`target/debug/examples/ifds_compare`, using the original function name,
`--format text`, and a separate fresh `--evidence-out` for each example. All
commands finished with exit code 0. Fresh artifacts were saved separately in
`/tmp/repo-memory-attribution-batch-wbus6ua4`; the archive was not overwritten.

For each pair, the `analysis` and `comparison` fields of the new full JSON were
checked for exact equality with the corresponding archived evidence. Regions,
result assessments, and previously verified change indicators therefore remained
unchanged. The new attribution adds causes and references to them; full and
compact reports now use schema 3, while the analysis still uses schema 4. Readers
in the current engine reject schema 2 reports. A new run requires a separate
evidence directory.

All 267 Rust tests, `cargo fmt --check`, and
`cargo clippy --locked --offline --all-targets -- -D warnings` passed. The 45
concrete JS scenarios were not executed again during this rerun; their previous
results remain in the original report. Matching structured comparisons do not
provide new proof of equal or unequal runtime values.

## Fixed

In [example 3](03-guard-version/before.ts), the explanation of return selection
shows `!first -> second`. Backward traversal of condition dependencies finds the
changed assignment on line 4. One confirmed cause is linked to all four
control-change regions: the return computation changes in two and remains
unchanged in two. The effect conditions `first && second` and
`!first && !second` are preserved.

Beginning of the actual stdout from the new run:

```text
Function result: versioned
Edit: (!first) -> second (before snippet.ts:4; after snippet.ts:4)
Control: !first -> second
Return source: b - a
  Before: under !first
  After: under second
```

Here, `Edit` expands assignment dependencies through input parameters rather
than quoting the source line `decision = !decision`. The evidence preserves
nodes, locations, and paths from the condition to the original assignment.
Example 3 no longer contains `Edit attribution uncertain` messages. For the
other nine examples, stdout matches the archive after excluding the `Evidence:`
line.

## Remaining work

| Priority | Examples | Work and acceptance criteria |
|---|---|---|
| 1 | 6, 7, 8 | Show a single established unchanged region in the short report: `!enabled`, `a && !useSaved`, and `skip`, respectively. It currently exists in JSON but disappears from the text. |
| 2 | 5, 9 | Explain the specific `Unknown` reason: an unknown call or unsupported `while`, with its source location. The diagnostics already exist in the evidence. |
| 3 | 4, 6, 8 | Extend attribution and grouping for operand swaps, replacement of conditional sources, and arithmetic-tree reassociation. There are currently 4, 8, and 2 uncertain-attribution messages, respectively. Show a shared confirmed cause once while preserving operand roles, write versions, and separate effects. |
| 4 | 7 | Distinguish nullish from falsy for `??`: `0` preserves `a`, while `null` selects `b`. Separately investigate establishing the common return of `100` under `!useSaved`, independently of the previous selection of `selected`. Currently, `Unknown under !a` covers both values of `useSaved`. |
| 5 | 1, 10 | Explain `Coverage: Partial` more clearly: unknown runtime equality and partial source alignment do not negate established unchanged or changed data flow. |

Loop support in example 9 is a separate analyzer extension. For now, explaining
this limitation precisely while retaining `Unknown` is sufficient.

Additional presentation improvements: explain the `10/20` selection in example 2
and exclusion of the uncalled `helper` edit in example 10. No errors in identifying
changed data flow were found in these two checks.

The new attribution confirms one unambiguously matched source of a condition
change. Multiple independent edits, ambiguous sources, unknown calls, and
incomplete paths remain uncertain; one new `causes` record does not establish
that the other attribution cases are solved.
