# FR003 — Function result reports and end-to-end acceptance

Status: implemented for the first synchronous single-function subset.

Dependencies: [FR002](002-result-comparison.md),
[K041](../kickoff/041-compact-source-report.md).

Source: [function result specification sections 5–8](../../docs/ifds/FUNCTION_RESULTS.md).

Fixture examples: [FR003 acceptance test sketches](TEST_EXAMPLES.md).

## Scope

- Expose the additive function-result API and an explicit target-selection mode
  in the comparison example command. Preserve the existing positional binding
  invocation. Select a function with `--function <name>` and, for an explicitly
  renamed counterpart, `--after-function <name>`. A name must identify one
  function in its snapshot; ambiguous names fail with an actionable error.
  Exact source-span selection remains available in the public API. Document the
  complete command invocation when shipped.
- Assemble separately versioned full evidence and compact/text projections from
  the same result facts, with deterministic identities and resolvable references.
- Explain changed conditions and their before/after results under shared inputs;
  show complete before/after selection conditions for affected return choices,
  factor proven shared guards into an explicit common context for compact output,
  and preserve equal and unknown relations with their scope and supporting evidence.
  Text follows [review 003](../function-result-review/003-compact-readable-text.md):
  ordinary equal regions stay hidden, established equal-result control changes
  remain visible once, and completely equal results without a control change get
  a scoped summary. Coverage is always shown; uncertain edit attribution is
  marked once per visible finding.
- For pure conditions over paired symbolic inputs, show the condition edit and
  any established change to the returned result without requiring an explicit
  Boolean entry domain. Keep a condition edit visible if the result stays equal;
  show uncertainty if the return effect cannot be established.
- Round-trip typed before/after entry-assumption expressions and show changes to
  query scope and its common/one-sided domains separately from source-code edits.
  Entry-domain restrictions are optional API inputs, not required command flags
  for the basic function comparison.
- When entry domains have no common inputs, display each side's guarded result
  flow under its own assumptions in text and compact JSON, followed by an
  explicit `no common inputs` notice. Never render a before-to-after result
  arrow or a result relation for this case.
- Promote independent end-to-end fixtures for every required example and preserve
  existing binding selectors, APIs, output modes, and fixture interpretation.

Excludes silently changing the existing command's binding argument into a function
selector, promoting planned language capabilities, or claiming measured precision
from synthetic acceptance cases.

## Required unit tests

| Test suffix | Required evidence |
| --- | --- |
| `api_round_trip` | A real assembled report preserves target, inputs, typed before/after assumption expressions, result regions, relations, evidence, coverage, and uncertainty through serialization. |
| `text_json_parity` | Text and compact JSON make the same condition/result claims without requiring Boolean entry domains for pure guards, including complete before/after selection conditions for affected return choices, guard edits with their return consequences, changed query-scope expressions and one-sided domains, side-by-side guarded flows with no cross-version arrow when the common domain is empty, `changed` computations without value evaluation, equal results with changed control, and unresolved comparisons. |
| `deterministic_evidence` | With deterministic clock/stat inputs, independent processing order preserves canonical output; every compact reference resolves to the exact full report. |
| `compact_grouping` | The enabled/ready example has one changed-result finding with shared controls; the status example shows one common context, a before/after condition table for `"ok"` and `"pending"`, and a concise effect statement. Context plus relative conditions recovers the complete rules. Proven unchanged choices remain in JSON and ordinary equal regions stay hidden in text; unknown results and incomplete coverage remain visible. Equivalent facts do not expand per witness. |
| `attribution_and_scope` | Findings link established contributing edits and source locations; uncertain attribution stays explicit. Open caller context is not rendered as closed value lifecycle. |
| `coverage_and_truncation` | Analysis, alignment, input mapping, result comparison, and presentation coverage remain distinct; omitted groups are counted and retrievable. Empty partial results never mean unchanged behavior. |
| `binding_compatibility` | Existing binding query/report schemas, full traversal, compact source output, and positional command invocation retain their previous semantics; explicit `--function` selection cannot reinterpret a binding name, and ambiguous function names fail. |

## Verification

Run `cargo test --locked --offline --lib ifds_fr003_ -- --list`, then
`cargo test --locked --offline --lib ifds_fr003_` and the
[feature completion gate](README.md#completion-gate).

Add independently authored function-result fixtures under the IFDS integration
suite, including all required semantic examples and selector/limit cases from the
specification. Run `cargo test --locked --offline --test ifds` and
`cargo test --locked --offline --test ifds_function_results`. Exercise the real
example command against local before/after fixtures in text, compact JSON and full
JSON modes, checking saved evidence and existing binding invocation compatibility.
Do not mark this feature delivered until FR001–FR003 and their integration gates pass.
