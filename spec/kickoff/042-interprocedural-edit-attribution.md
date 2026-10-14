# K042 — Function-result edit attribution across matched calls

Status: planned. Roadmap stage: 4, function-result reporting extension.

Dependencies: [K011](011-node-alignment.md),
[K018](018-interprocedural-calls.md),
[FR003](../function-results/003-reports-and-acceptance.md).
Can follow those tasks directly; K019 recursion, K020 caller continuation, and
K033 ambiguity refinement are not prerequisites.

Sources: [matched-call evidence](../../docs/ifds/INTERPROCEDURAL_CALLS.md),
[function-result reporting](../../docs/ifds/FUNCTION_RESULTS.md), and the
[existing attribution contract](../function-result-review/001-report-values-and-attribution.md#attribution-and-evidence).

## Problem and reproduction

The function-result dependency graph follows supported calls across files, but
the unique-edit attribution pass still checks dependency positions relative to
the selected function's declaration. In
`src/ifds/function_result_report/attribution.rs`, `same_relative_lines` requires
each dependency's file to equal the selected function's file and its lines to
start at or below that function's declaration. `dependency_edits` aborts when
this check fails. A supported dependency in a different function can therefore
prevent attribution even when all source alignment is complete. A helper above
the selected function in the same file can fail for the same reason.

The [four-module example](../../examples/interprocedural_files/README.md) selects
an unchanged `buildMessage` in `main.ts`. It calls `makeGreeting` in `greeting.ts`,
which calls `greetingPrefix` in `prefix.ts`, and passes the result to
`addPunctuation` in `punctuation.ts`. Only the literal in `prefix.ts:2` changes:
`"Hello, "` becomes `"Welcome, "`.

The current report establishes the changed computation
`(("Hello, " + "Ada") + "!")` → `(("Welcome, " + "Ada") + "!")`, with complete
result comparison, source alignment, and data flow. Nevertheless, it sets
`attribution_certain: false` and prints `Edit attribution uncertain`. Traversal
can stop at an unchanged dependency in `punctuation.ts` before reaching the
changed literal in `prefix.ts`.

## Scope

- Extend unique contributing-edit attribution to the supported nonrecursive,
  synchronous matched calls from K018, including nested calls and helpers in the
  same or different files. Keep the selected outer function as the query target.
- Use established cross-snapshot operation, declaration, and call-site
  correspondence. Evaluate source positions within the dependency's own
  procedure and snapshot file, rather than relative to the selected function.
  Reuse existing alignment evidence or expose the missing evidence explicitly;
  do not assume complete aggregate coverage supplies every required node pair.
- Preserve snapshot, procedure, binding/write version, operand role, and ordered
  call context when pairing dependencies. Different local node IDs or identical
  source positions in separate modules must not collide. Trace actual/formal
  and return/result transfers without presenting synthetic transfers as edits.
- Establish attribution only when one uniquely matched contributing source
  operation explains the changed value dependency across every observation and
  input region represented by the finding. Retain the existing conservative
  treatment of changed control, unresolved dependencies, and multiple edits.
- Count repeated expansions of one matched source operation as one edit while
  retaining each invocation's evidence and affected regions. Do not merge
  independent call sites, distinct source operations, or equal-looking literals
  merely because their text matches.
- Ignore changed definitions that do not reach the selected result, including
  overwritten values and unrelated calls. Establish the contributing edit from
  the compared dependencies; the number of edited files or lines is not proof.

Excludes recursion expansion, new dispatch resolution, captured/heap effects,
new language support, arithmetic or string evaluation, and historical bug-cause
inference. Unsupported or ambiguous relationships remain explicit boundaries.
Do not simply remove the file/line checks without replacing their correspondence
guarantees. No change to binding-query semantics is required.

## Report contract

For the four-module example, establish `attribution_certain: true` and identify
the literal replacement at the before/after locations in `prefix.ts:2` as the
contributing edit. Show the edit once in concise text, connect it to the affected
outer return, and keep the full call/dependency chain retrievable in the evidence.
The unchanged `main.ts`, `greeting.ts`, and `punctuation.ts` must not be described
as edited causes merely because they contribute to the result.

Text and compact JSON must derive attribution from the same structured facts.
Keep resolvable references to both source operations, their call contexts, the
outer return observations, and the affected comparison regions. References must
validate against full evidence; a bare file/line label is insufficient. If the
existing report model cannot carry this proof, extend and version the affected
public schemas, serialization, and validation together. Preserve archived report
provenance and document any schema change.

Remove `Edit attribution uncertain` only for findings whose attribution is
established. Preserve it for ambiguous or unsupported findings, even when another
finding is certain. Complete source alignment and a known result are not by
themselves proof of edit attribution. Conversely, an unrelated unresolved region
must not suppress a separately established attribution.

Keep existing result assessments, comparison regions, entry assumptions, data-flow
facts, coverage, and scope boundaries. In particular, the example remains
`Changed`; certain attribution must not upgrade it to `Different` or introduce
evaluated string results. K042 does not repair unrelated result-comparison flags.

## Required unit tests

Use the `ifds_k042_` prefix. Author expected edits, call contexts, source spans,
regions, and forbidden claims independently from analyzer output.

| Test suffix | Input and exact assertions |
| --- | --- |
| `four_module_literal_edit` | The reproduction has one changed literal in `prefix.ts:2` and an unchanged outer function. Establish certain attribution to that literal, retain all matched call hops, preserve `Changed` and all three complete coverage dimensions, and omit the uncertainty marker for this finding. |
| `same_file_helper_before_target` | A uniquely matched helper above the selected function changes its returned literal. Attribute that helper edit without requiring its span to lie inside or below the selected declaration. |
| `shifted_helper_positions` | Comments or unrelated lines shift a uniquely aligned helper and the target. Attribute the same operation at its actual before/after positions without requiring equal absolute or target-relative lines. |
| `nested_argument_return_transfers` | A changed helper operand reaches the outer result through actual/formal and return/result transfers. Preserve operand roles and write versions, and attribute the source operation rather than a synthetic transfer. |
| `repeated_callee_contexts` | Two contributing invocations of one edited helper yield one source-edit identity with distinct call contexts and correctly scoped affected observations. A noncontributing invocation supplies no cause evidence for the return. |
| `identical_spans_distinct_modules` | Two modules contain equal-looking operations at identical line/byte positions. Only the uniquely paired contributing operation is attributed; matching text or local IDs do not combine the modules. |
| `overwritten_and_unrelated_edits` | A uniquely contributing helper edit remains attributable despite other file edits that are overwritten or do not reach the result. With only those noncontributing edits, no changed-result cause is invented. |
| `multiple_contributing_edits` | Two distinct changed operations reach the same finding. Preserve changed dependencies and uncertain attribution; do not choose the first edit or claim one uniquely responsible operation. |
| `ambiguous_callee_or_call_pair` | Unresolved operation, callee, or call-site correspondence prevents certain attribution for the affected finding, even with identical text or otherwise complete coverage. Preserve the supported dependency prefix. |
| `unsupported_boundary_and_limits` | Unknown dispatch, unsupported callee effects, recursive expansion boundaries, or exhausted evidence budgets on the contributing chain do not yield certain attribution. Keep existing reasons and limits visible. |
| `independent_findings` | One finding has a unique supported helper edit and another has unresolved dependencies or changed control. Keep the first certain and the second uncertain, with separate evidence and conditions. |
| `report_evidence_parity` | Full/compact JSON round-trip and text agree on edit identity, locations, contexts, and certainty. Reject references to a different snapshot, call, source operation, or comparison region; preserve existing local attribution behavior. |

## Integration and verification

Add an end-to-end case in `tests/ifds_calls.rs` named
`ifds_k042_real_provider_cross_file_attribution`, using the pinned TypeScript
provider on an immutable tiny project with the four authored modules above.
Assert certain attribution and resolvable source/call evidence through the public
metadata-backed function-result API. Assert the existing result relation and
coverage separately from attribution. Do not run the analyzed application or
its package scripts as part of this gate.

Run:

```sh
cargo test --locked --offline --lib ifds_k042_ -- --list
cargo test --locked --offline --lib ifds_k042_
cargo test --locked --offline --test ifds_calls
```

Verify every named unit test is present and none is ignored, then run the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
Regenerate reviewable text and full/compact evidence for the multi-file example
with recorded provenance, and review its attributed operation against the
authored source change rather than accepting a generated golden as the oracle.
