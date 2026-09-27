# FR003 — Function result reports and end-to-end acceptance

Status: planned.

Dependencies: [FR002](002-result-comparison.md),
[K041](../kickoff/041-compact-source-report.md).

Source: [function result specification sections 5–8](../../docs/ifds/FUNCTION_RESULTS.md).

Fixture examples: [FR003 acceptance test sketches](TEST_EXAMPLES.md).

## Scope

- Expose the additive function-result API and an explicit target-selection mode
  in the comparison example command. Document exact invocation syntax when shipped.
- Assemble separately versioned full evidence and compact/text projections from
  the same result facts, with deterministic identities and resolvable references.
- Explain changed conditions and their before/after results under shared inputs;
  show complete before/after selection conditions for affected return choices,
  factor proven shared guards into an explicit common context for compact output,
  and expose equal and unknown relations with their scope and supporting evidence.
- Promote independent end-to-end fixtures for every required example and preserve
  existing binding selectors, APIs, output modes, and fixture interpretation.

Excludes silently changing the existing command's binding argument into a function
selector, promoting planned language capabilities, or claiming measured precision
from synthetic acceptance cases.

## Required unit tests

| Test suffix | Required evidence |
| --- | --- |
| `api_round_trip` | A real assembled report preserves target, inputs, result regions, relations, evidence, coverage, and uncertainty through serialization. |
| `text_json_parity` | Text and compact JSON make the same condition/result claims, including complete before/after selection conditions for affected return choices, guard edits with their return consequences, `changed` computations without value evaluation, equal results with changed control, and unresolved comparisons. |
| `deterministic_evidence` | With deterministic clock/stat inputs, independent processing order preserves canonical output; every compact reference resolves to the exact full report. |
| `compact_grouping` | The enabled/ready example has one changed-result finding with shared controls; the status example shows one common context, a before/after condition table for `"ok"` and `"pending"`, and a concise effect statement. Context plus relative conditions recovers the complete rules. Proven unchanged choices are summarized, while unknown results and incomplete coverage remain visible. Equivalent facts do not expand per witness. |
| `attribution_and_scope` | Findings link established contributing edits and source locations; uncertain attribution stays explicit. Open caller context is not rendered as closed value lifecycle. |
| `coverage_and_truncation` | Analysis, alignment, input mapping, result comparison, and presentation coverage remain distinct; omitted groups are counted and retrievable. Empty partial results never mean unchanged behavior. |
| `binding_compatibility` | Existing binding query/report schemas, full traversal, compact source output, and command invocation retain their previous semantics. |

## Verification

Run `cargo test --locked --offline --lib ifds_fr003_ -- --list`, then
`cargo test --locked --offline --lib ifds_fr003_` and the
[feature completion gate](README.md#completion-gate).

Add independently authored function-result fixtures under the IFDS integration
suite, including all required semantic examples and selector/limit cases from the
specification. Run `cargo test --locked --offline --test ifds`. Exercise the real
example command against local before/after fixtures in text, compact JSON and full
JSON modes, checking saved evidence and existing binding invocation compatibility.
Do not mark this feature delivered until FR001–FR003 and their integration gates pass.
