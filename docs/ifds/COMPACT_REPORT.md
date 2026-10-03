# Compact source and logic change report

Status: implemented for the supported subset. Owner: [K041](../../spec/kickoff/041-compact-source-report.md).

This extends [SPEC sections 6.4 and 7](SPEC.md#64-grouping-and-required-summary-content).
It defines the default explanation for people and agents. Full graphs, typed deltas,
witnesses, and lifecycle records remain the evidence model required by SPEC.
Implementation must not reduce analysis scope to make the explanation smaller.

This contract remains centered on a selected binding. The planned
[function result report](FUNCTION_RESULTS.md#5-report-and-presentation) reuses its
shared guards, dependencies, evidence references, and text/JSON parity principles
in a separately versioned schema. That mode selects all normal results of a
function and adds same-input result relations; it does not reinterpret existing
`VariableSourceReport` observations as a complete function-result analysis.

The API returns `(VariableFlowReport, VariableSourceReport)` from
`analyze_variable_flow_reports`. The existing `analyze_variable_flow` API keeps
returning the full report. `VariableSourceReport` has its own schema version and
contains sources, typed guard clauses, observations, source selections, overwrite
rules, semantic findings, and separate analysis/comparison/presentation coverage.
Its `full_report_id` is the deterministic FNV-1a digest of canonical full JSON with
`human_summary` cleared, so the summary can refer to the report without making
the digest circular. Existing runtime `stats.elapsed_ms` participates in that
digest, so independent analyses of identical queries can have different IDs.
Each evidence reference names a section and zero-based index
in that exact full report. The example command writes the full JSON sidecar for
`text` and `compact-json`; `full-json` writes complete evidence to stdout.

## Human text modes

Following [005](../../spec/function-result-review/005-binding-human-report.md),
binding `text` describes a shared change once and identifies its affected uses.
Both snapshots' positions remain explicit, including moved uses. Observations
with distinct conditions or uncertainty retain those distinctions; this is a
text presentation change, not a change to structured observation grouping.

Short text retains computations and exact operand roles. An unchanged transparent
copy may be omitted only with an unambiguous, exhaustive single origin and no
hidden condition or binding-version distinction. Selection explanations show the
change plus necessary fallback, reaching conditions, and overwrite priority.
An assignment's enabling condition is distinct from the condition under which
its value reaches a particular input. Standalone write edits without established
impact on uses remain in detailed text and evidence. An empty visible finding
list states only the absence of established changes at uses within the scope.
Binding text does not establish equality of whole function results.

`--verbose` retains the previous level of detail and adds explicit source chains,
positions, and selection rules. Both modes preserve uncertainty, entry
restrictions, independent analysis/comparison coverage, limits, presentation
omissions, open boundaries, and the exact evidence link. The flag works only with
text format, for either target; JSON combinations fail before source/evidence I/O.

For function targets, a resolved change to return dependencies is `Changed`
without runtime type information or a proof of unequal values. When the return
expression stays the same through a saved copy, text shows
`Changed computation under <condition>` with the expanded before/after
dependencies and the return they reach. This requires complete input
correspondence and resolved dependencies through normal completion; transparent
copies are expanded using paired input names. Unresolved calls, input pairing,
and result selection retain `Unknown` and their reasons.

When resolved return dependencies match but runtime value equality lacks a type
proof, function text states `Unchanged data flow` with the expanded computation
and `runtime value equality unresolved`. This requires complete input pairing,
established path selection and normal completion, and identical entry assumptions.
The structured assessment remains `Unknown`, so result-comparison coverage is
still partial. Matching source text or unresolved calls do not establish this
dependency identity.

Short function text groups established unchanged dependency regions by their
union, even when different computations supply their results. For example,
three unchanged paths in a two-guard overwrite become `!first || !second`.
The summary preserves whether runtime equality is proven everywhere, unresolved
everywhere, or unresolved in part of that union. Verbose text retains the separate
computations and conditions. Unknown dependencies and omitted presentation groups
cannot enlarge this union; structured regions and evidence remain unchanged.
An individual established equal region is shown as well, including an early
return alongside changed computations. Verbose text keeps the separate return
computations and conditions rather than replacing them with the short union.

Function result regions carry two independent input predicates: `values` records
truthiness and `nullish` records whether an input is `null` or `undefined`.
Unlisted predicates are unconstrained. A nullish input is always falsy; a truthy
input is non-nullish. Impossible truthy/nullish combinations are excluded.
Resolved `IsNullish` dependencies on positional inputs and their captured copies
are partitioned separately from truthiness, so `0`, `false`, and empty strings
retain the left operand of `??`, while `null` and `undefined` select its fallback.
Arbitrary unresolved computed predicates and unknown calls retain uncertainty.
The comparison budget counts predicate dimensions and remains bounded at 12.

Coverage separately records `data_flow` and `result_comparison`. Complete data
flow requires established return selection/completion and dependencies across
the common input domain, complete input correspondence, and unexhausted analysis
and comparison budgets. `result_comparison` can remain partial solely because
runtime equality of unchanged computations is unresolved; text explicitly
explains this case. Source alignment and presentation completeness remain
independent coverage dimensions.

A proven permutation of unique pure paired-input guards is shown once as
`Control: guard order ... -> ...`. Guard versions and outcomes must match;
unsupported predicates and ambiguous correspondence retain uncertainty.
When a uniquely anchored pure guard switches to another paired input, function
text first explains the selected return source and its full reaching conditions
on each side: for example, `Control: !first -> second`, `Return source: b - a`,
`Before: under !first`, `After: under second`. These side-specific selection rules
are separate from same-input effect regions such as `first && second`. Guard
versions, observation topology, return computations, and contributing writes must
match after pairing that one guard; enclosing conditions remain in the reaching
rules. Unknown completion, ambiguous source anchors, changed returned computations,
and incomplete presentation suppress this explanation. It does not broaden the
same-input effect regions or change their assessments.
Short text omits the return-choice table when expanded computations already
explain every changed effect with its full same-input condition.

Function edit attribution also follows resolved dependencies behind unchanged
return expressions. One source operation shared by all affected return paths
can establish attribution even when there are several observations. Pairing
requires matching relative source lines, node shapes, operand roles, guards, and
positional inputs; unresolved dependencies, changed entry assumptions, or several
candidate edits retain uncertainty. Text shows the unique edit once with both
source positions. This local attribution does not imply complete alignment of
every source operation in the file.

Function reports also attribute a single changed guard through its
reaching assignments. The full report's `causes` records paired before/after
observation and guard indices, branch IDs, expanded condition expressions, and
the source edit's dependency path, node, operation, location, and expanded input
expression. These dependency paths start at the selected guard's condition root.
Each cause lists absolute `comparison.regions` indices; full findings link to
causes by index and compact findings by `EvidenceRef` with section `causes`.
Those regions may include control changes preserving the returned computation.

Attribution requires complete path/dependency evidence, paired inputs, identical
entry assumptions and return topology, and one changed guard with one uniquely
anchored source edit. Local assignment comparisons stop at their input reads'
reaching definitions, so an upstream edit copied through several variables is
attributed to its original assignment. Captured variable versions are preserved;
subsequent overwritten or unused assignments are excluded. Multiple candidate
edits, ambiguous same-line anchors, changed return computations, unknown calls,
and exhausted analysis/comparison budgets retain uncertainty. Direct predicate
edits are supported when no upstream assignment changed. Text prints the shared
edit once with both source locations; rendering does not mutate evidence.
Full and compact function reports use schema 4; analysis remains schema 4.
Older report schemas are rejected, and historical archived reports are unchanged.

When one changed read in a return expression has the same resolved dependency
on several paths, short text shows that computation and its shared writes once.
Each path then shows the surrounding return expression using the read's source
name, its exact condition, and the remaining before/after writes. Grouping
requires a unique shared edit, identical shared dependencies and writes, and all
grouped regions to survive the presentation limit. Independent edits and
ambiguous repeated reads retain separate effects. Verbose text retains the full
before/after computation for every path; JSON and evidence are unchanged.

The API's `render_text(&full)` and `render_verbose_text(&full)` consume the same
compact facts and exact full report. These methods now require the full evidence
to expose its limits and boundaries and verify report identity. The original
no-argument renderer is available as `render_legacy_text()`; full JSON keeps that
legacy `human_summary` and its existing custom evidence-path handling. Full and
compact JSON structures, schema versions, IDs, and evidence remain unchanged by
choosing either human mode.

## Questions answered

For one selected binding, answer:

1. Which assignments and upstream inputs can supply its value at the relevant use?
2. Which guards control those assignments or whether that use is reached?
3. Which sources, expressions, selection rules, or uses changed between revisions?
4. Which of these answers are established, modeled, or unresolved?

Do not render a sentence for every graph edge, delta kind, or execution path.
`NodeAdded`, `WriteAdded`, `FlowAdded`, and `ValueSourceChanged` can all describe
one added source. They must contribute evidence to one source finding rather
than become four repeated explanations.

## Source meaning and observation points

A variable does not have one source set valid everywhere in a function. Attach a
source set to an observation: a read, an exact consumer input, or a scope boundary.
The selector still identifies the declaration; it does not implicitly select only
the last return. Compute the full supported query slice as before.

- Identify an immediate writer by its aligned operation, RHS expression, and source
  location. Equal RHS values at different writes are not the same source.
- Identify external inputs separately: parameters, properties, modeled call results,
  or other supported boundaries. A controller such as `flag` is not a value origin
  merely because it decides whether `x = 2` executes.
- Preserve upstream dependencies through intermediate writes. For `x = p; y = x + 1`,
  retain both the immediate writer of `y` and its dependence on `p`. Shared source
  chains are referenced once; they are not expanded into every possible path.
- A write overwritten before a particular use is not a source at that use. Copies
  made before the overwrite may still carry it to other uses.
- For `return x + 4`, distinguish the source of the `x` operand from the literal `4`
  and from the computed return result. Do not claim that a write to `x` directly
  supplies the whole return value or infer a constant result without evidence.

Include source state and changes at affected observations. Group observations only
when their before/after source facts, relevant control facts, and evidence status
are equivalent. Keep their individual identities and locations available by
reference. Explicitly retain new early returns, removed uses, changed expressions,
and distinct caller contexts. Unchanged intermediate hops can be represented by
shared dependency references; this does not discard downstream analysis.

When no observation is affected, say so within the declared scope. A changed write
that is overwritten before all uses may be reported once as an edited write with
no established effect on those uses. Do not present it as an added reaching source.

## Controls and assignment precedence

Separate these facts instead of printing their Cartesian product:

- **Assignment guard:** what enables this writer, e.g. `flag` for `x = 2`.
- **Use guard:** what enables the observation, e.g. `!flag` for a return after an
  early exit under `flag`.
- **Overwrite precedence:** which later applicable writer replaces an earlier one
  before the observation. Report this only when supported by analysis evidence.

For supported sequential overwrites, a source set may use an ordered-overwrite
selection model: start with its fallback source, apply the guarded writers in
execution order, and take the last applicable writer. This is a compact selection
rule, not a list of execution traces. Represent shared order and guard facts once.
Nested `if/else` uses shared guard structure; mutually exclusive writes do not
become an invented precedence relation.

Assignment guards alone are insufficient. In `if (flag) x = 2; if (flag2) x = 3`,
claiming that `flag` guarantees the value 2 is false. The report must preserve the
later overwrite by `x = 3`.

Guard identities refer to the resolved binding/value version at the guard use,
not just its display name. Reassignment, shadowing, short-circuit side effects,
and different caller contexts must not be merged through text equality.

Supported simplification may remove redundant guards from a source's selection
rule. For unchanged independent Boolean inputs, `(flag && flag2) || (!flag && flag2)`
is equivalent to `flag2`. Simplification requires a proof within supported rules;
it cannot establish previously unresolved reachability. Do not parse condition
display strings as the semantic model or drop conjuncts by string replacement.

Do not defer compact reporting for supported K014 cases until K032. Conversely,
K041 does not add support for K015 expressions, general predicate solving, loops,
or other unsupported language features. Complex supported conditions can be stored
once as a shared expression with a concise label and explicit detail reference.
If a compact rule cannot be established, mark selection unresolved or link to its
established exact condition; never replace it with an inaccurate simple flag list.

## Changes and uncertainty

Each finding identifies an affected observation/source, before and after facts,
and evidence references. The semantic change categories are:

- source added or removed at this observation;
- retained source with changed producing expression or upstream input;
- retained sources with changed guard, overwrite order, or observation guard;
- observation added, removed, or changed;
- comparison unresolved for a specified source, guard, or observation.

A guard-only change must say that source selection changed while the sources
remain. Reordering the two writes in the example changes precedence despite an
unchanged source set and guard names. An inserted `x = x` remains a new write even
if it preserves the value. A changed operator with unchanged inputs is still an
expression change. These distinctions must survive compression.

Group related structural facts into a single finding and reference all supporting
deltas. Do not guess which edit caused an effect. The same facts drive both text
and agent JSON; the agent must not need to parse prose to recover them.

Expose analysis coverage, comparison certainty, and presentation coverage
separately. In particular, complete per-snapshot graphs with `AmbiguousMatch` do
not establish a complete comparison. Unknown origins remain unknown, not absent.
Boundary assumptions and affected unknown regions appear once, with references
from their affected findings. Do not claim a complete value lifecycle merely
because analysis is complete within a function.

## Compact contract and output modes

Introduce a separately versioned `VariableSourceReport` projection built from the
structured evidence, with these required concepts (field spellings can be finalized
when implementing K041):

| Concept | Content |
| --- | --- |
| Identity and scope | Compact schema version, analysis version, snapshot pair, selected declaration, entry assumptions and declared scope. |
| Sources | Unique writer/input definitions, expression, before/after locations, evidence category, and references to upstream sources or computations. |
| Controls | Shared guard definitions with value/binding identities and before/after expressions. |
| Observations | Exact use/input identity, locations, and references to before/after source sets and selection rules. |
| Selection rules | Guarded writers, supported overwrite precedence, use guards, or referenced exact conditions; explicit unresolved status where necessary. |
| Changes | Deduplicated semantic findings referencing the sources, controls, observations, and supporting evidence. |
| Limits and evidence | Analysis coverage, comparison uncertainty, presentation completeness, models/boundaries, and resolvable detail references. |

IDs are report-local or pair-local as appropriate, not persistent cross-history
identities. Sort deterministically; source position is a location, not identity.
Do not repeat entire source expressions and paths inside every finding. Keep an
evidence reference tied to the exact snapshot pair and full-report identity so it
can be resolved without a new, potentially different analysis run.

Provide three explicit output modes:

- `text`: concise rendering of the compact facts, intended as the default for a
  human-facing command once this contract is adopted;
- `compact-json`: the same semantic facts for agents, without embedded full graphs
  or witness lists;
- `full-json`: the complete existing evidence report, available explicitly.

Preserve the existing full analysis API. Adding the compact API and changing the
example command's default are documented interface changes, not silent changes to
the meaning of `VariableFlowReport`. Provide an explicit `full-json` migration
command for existing scripts. If `human_summary` is retained inside the full report,
render it from the same compact facts rather than a separate delta-to-text loop.

Size should grow with distinct sources, meaningful changes, and observation groups,
not the number of feasible execution paths. Do not impose a word cap that silently
loses facts. If a presentation budget omits groups, expose omitted counts and a
continuation/detail reference, separately from analysis truncation. Uncertainty
must remain visible. Small examples below must fit without truncation.

## Required examples

### Two sequential conditional writes

```typescript
// Before
let x = 1;
return x;

// After
let x = 1;
if (flag) x = 2;
if (flag2) x = 3;
return x;
```

With independent unchanged Boolean input values, the complete text can be:

```text
x at return x (main.ts:5)
Before: x = 1.
After: x = 1 as fallback; added x = 2 under flag; added x = 3 under flag2.
Precedence: x = 3 overrides x = 2 when both writes apply.
Scope: f; source comparison complete.
```

Writer locations belong in the source definitions; a UI or expanded text may show
them inline once. These lines are illustrative rendering of structured facts.
No separate sentences for literal-to-write edges, `may_write`, each branch witness,
or a second enumeration of the same source-set change are needed.

### Guard changes without new sources

Changing `if (flag) x = 2` to `if (!flag) x = 2` retains the two source definitions
and reports one selection change: the guard of `x = 2` changed `flag -> !flag`.
Preserve the fallback `x = 1`; do not report it or the retained write as newly added.

### Conditional early return

After inserting `if (flag) return x` after `x = 2` in a reassignment chain, report
the new observation supplied by `x = 2` under `flag`, and that later writes and the
existing return are now reached only under `!flag`. Group the shared continuation
guard once. Preserve the old final return exactly; do not change sample code to
avoid an alignment limitation. Two indistinguishable `return x` sites require
explicit matching uncertainty unless the correspondence can be established.

### Upstream change and an overwritten source

For `x = p; y = x; x = 3; return y`, replacing `p` with `q` changes the origin of
the returned copy even though `x` is later overwritten. In `x = p; x = 3; return x`,
the same edit does not change the return's source. Preserve this distinction and
the direct writer versus upstream input distinction in compact JSON and text.

## Implemented projection steps

1. Build compact source/observation identities from alignment, slices, provenance,
   typed deltas, and boundaries; do not summarize `human_summary` strings.
2. Expose typed control and overwrite evidence needed for selection rules. Existing
   textual conditions and omission of unguarded `may_write` edges are insufficient
   by themselves. If necessary extend report assembly evidence without guessing
   order from line numbers or altering source claims.
3. Group equivalent source facts and affected observations; deduplicate explanations
   using semantic identities and evidence groups, not sentence similarity.
4. Produce both compact JSON and text from this model. Keep full evidence accessible
   through explicit output and stable references within the snapshot pair.
5. Validate against independently authored expectations in K041, including unknown
   comparisons and repeated returns. Benchmark the full evidence under its existing
   contract separately from presentation compactness.
