# Automated benchmark specification

Status: planned test system and acceptance criteria. No fixtures, runner, or
precision measurements for IFDS exist yet. Existing `tsx_diff` regression tests
exercise syntax-level changes and are not evidence of IFDS correctness.

## 1. Test contract and independent expectations

Each case supplies a small repository before and after an edit, its diff, one
selected binding, the enabled capabilities and summary versions, and reviewed
expected results. Tests run offline and must not fetch branches or GitHub PRs.
Use immutable revisions and retain required upstream attribution for copied code.

Planned fixture layout:

```text
tests/ifds/fixtures/<case>/
  manifest.json
  before/                   complete small project or pinned dependency slice
  after/
  change.diff
  expected.json
  README.md                 reasoning, boundaries, provenance, license references
```

`manifest.json` declares schema version, case/category IDs, capability stage,
project configuration, snapshot hashes, selector spans, optional counterpart,
entry assumptions, model versions, deterministic resource limits, and source PR
metadata if relevant. Full-repository integration cases reference a locally
provisioned immutable checkout plus a content manifest. The measured test run
still requires no network or app execution.

`expected.json` must declare:

- Logical node labels and expected before/after source spans or bindings.
- Required node, write, flow, condition and value-source changes.
- Forbidden flows and changes, including the reason each would be incorrect.
- Retained relations that must not be mislabeled as added/removed.
- Complete supported upstream/downstream chains, including unchanged intermediate
  and final consumers outside the diff; expected consumer/input identities.
- Expected completeness, summary assumptions and uncertainty frontiers.
- Directional coverage and per-path ending kinds, separating proven no-further-use
  or execution exit from cycles, escaping returns, model boundaries, and unknowns.
- Witness requirements: valid call pairing, relevant guard outcomes, no killed
  definition reaching a read, and appropriate loop/exception paths.

Ground truth must be written/reviewed from program semantics before accepting
analyzer output. Do not regenerate expected JSON from the implementation under
test or approve snapshots merely because they stabilize. A different implementation
can help cross-check but is not automatically ground truth.

For small, finite fixtures, use an independent bounded interpreter/enumerator with
instrumented writes and reads to validate possible flows. A bound is an exact
oracle only where all executions in the declared input/loop domain are covered.
Hand-proven loop invariants and reviewed witnesses are needed for unbounded cases;
finite runtime observations alone cannot establish the absence of a static flow.

## 2. Scoring unit

Do not score true-negative pairs over every possible pair of nodes; that would
inflate an accuracy number in a sparse graph. Score positive reported flow-change
claims against canonical expected claims.

Each claim has a normalized identity:

```text
(query_binding, delta_kind, relation_kind,
 logical_source, logical_consumer, input_or_place_projection,
 changed_condition_or_origin, before_after_presence)
```

Fixture-specific labels normalize spans and IDs without using the implementation's
alignment algorithm as the oracle. Multiple witness paths for the same claim count
once. Direct writer-to-read changes and transitive origin-to-consumer changes are
different relation kinds and are scored separately as well as in the aggregate.
Generic class/function edit summaries are not additional flow claims.
Unchanged graph context and path-ending records have independent exact assertions;
they do not inflate the precision denominator or count as added/removed flows.
A fixture missing its required downstream chain fails even if its shorter delta
list happens to contain no false positives.

Count `supported` and `modeled` reported relations as positive predictions, with
their summary assumptions checked. An incorrect dependency remains a false
positive even if its prose says "may". Unresolved frontiers are separately scored
abstentions only if the output does not assert a concrete dependency. If the UI or
API promotes such a candidate to a flow claim, it enters the precision denominator.

```text
TP = correctly reported expected flow-change claims
FP = reported flow-change claims not justified by the oracle
FN = expected flow-change claims not reported, including abstentions
precision = TP / (TP + FP)
recall    = TP / (TP + FN)
```

Match relation direction, before/after classification, guards, source identity,
argument/property projection and required assumptions, not just matching endpoint
names. Code edits and slice-membership changes have their own exact structural
checks; they must not be used to inflate flow precision.

## 3. Acceptance gates and avoiding a quiet analyzer

1. Every promoted exact semantic fixture must match its required/forbidden claims
   and diagnostics. These fixtures are ordinary pass/fail regression tests.
2. Reported-flow precision must be at least 90% on the reviewed integration suite,
   both overall and for each supported complexity category with positive outputs.
3. Publish recall over all labeled expected claims, plus recall over currently
   supported capabilities. Report unsupported cases, abstentions, unresolved
   claims, and complete/partial query counts separately.
4. Empty predictions on a case with expected positive claims have precision `N/A`,
   recall zero, and fail the exact fixture. A suite cannot pass the precision gate
   by abstaining on every positive case. Unchanged negative cases with no output
   pass their negative assertion without contributing fictitious true positives.
5. Freeze the benchmark/capability manifest for an evaluation. Reclassifying failing
   syntax as unsupported or removing difficult cases requires explicit review and
   a visible report of lost coverage. Existing promoted coverage cannot silently
   regress. No aggregate recall target is yet agreed for future unsupported syntax.
6. Track unresolved cases as implementation work, not as completed coverage. Each
   stage must enable its required families and pass their exact fixtures.

Publish micro precision/recall, category results, raw TP/FP/FN counts, numbers of
independent programs and PRs, and model versions. Maintain development and held-out
integration cases. A measured 90% on this suite is not a universal guarantee for
unseen repositories or a calibrated probability for individual findings.

## 4. Required case families

Each promoted syntax family needs at least a positive flow, a negative flow, a
before/after change, and an uncertainty-boundary case. Include combinations as
complexity grows rather than only isolated syntax examples.

| Family | Required examples and failure checks |
| --- | --- |
| Initial values and writes | Changed literal or input initializer; added/removed assignment; `x = x`; copied value survives later overwrite; a write that is never read; binding selected on only one revision. |
| Expression provenance | Multiple operands, nested computations, templates; unchanged immediate edge with changed upstream expression; reassignment kills only its own place; no claim of proven concrete value inequality. |
| Full downstream extent | Section 6 case 10's origin edit propagates through unchanged copies/calculations to the return; do not stop at the first use, original-binding overwrite, or lexical scope if a derived value escapes. Keep all relevant consumers, including unchanged files reached by supported calls, but not unrelated later statements or sibling uses of an upstream input. |
| Lifecycle and boundaries | Kill one copy while another survives; kill the last carrier with no remaining control consequences; unread local value; path-specific early exits and cleanup; cyclic continuation; entry return with an out-of-scope receiver; modeled external sink; unsupported escaping capture/schedule; budget exhaustion. A boundary or fixed-point convergence must not be labeled runtime termination or proof of no further impact. |
| Binding identity | Same spelling in nested scopes; parameters versus locals; independent functions; declaration moves and line shifts; ambiguous counterpart; unrelated edits yield an empty flow delta. |
| Branches | One-sided and two-sided writes; nested guards; changed condition only; short-circuit skipped operand; early return; constant-false branch; contradictory guards produce a rejected or explicitly unresolved path. |
| Loops | Zero iterations versus `do/while`; loop-carried definitions; overwrite before read; nested loops; continue to correct update/condition; break target; backedge addition/removal; no infinite witness enumeration. |
| Calls | Argument copy, return value, changed callee return with unchanged call site; full argument/return-to-later-consumer chains; a selected callee local returned to a specifically included caller context; two callers with different inputs; nested calls; direct/mutual recursion; parameter reassignment does not mutate caller binding. |
| Modules | Aliases/re-exports and local shadowing; configuration-dependent resolution; missing body or declaration-only dependency; incompatible runtime target remains unresolved. |
| Projections and models | Separate object fields, literal keys/indices, URL/header input slots; known disjoint spread versus possible overwrite; known string transformation; unknown library return and unknown getter behavior. |
| Exceptions | Throw before assignment commit; catch-side overwrite; return/throw through finally; finally overrides pending completion; normal and exceptional return sites cannot be confused. |
| Await | Known request inputs before suspension; fulfilled/rejected continuation; suspended code with externally mutable captures remains uncertain; no invented dependency from request to concrete server response. |
| Mutation and aliases | Aliased property write; unaliased strong kill; ambiguous weak update; array writes and mutators; destructuring; bounded access path overflow; binding copy versus object alias. |
| Closures/classes | Captured write; per-iteration binding; lexical `this` in arrows; static/instance separation; callback invoked now versus stored; unresolved override dispatch; getters/setters with effects. |
| Other execution | Switch fallthrough; labeled exits; module cycles; generator yield/resume; iterator callbacks; async iteration; event ordering; modeled React state updates versus ordinary assignments. |
| Comparison | Source removed from the selected slice but still in repository; write target changed; inserted repeated call; equivalent formatting; incomplete-before must not imply a new flow; incomplete-after must not imply a removed flow. |
| Limits | Solver timeout, path-edge budget, missing summary, access-path bound, witness-only truncation versus graph truncation; none may be reported as a complete empty delta. |

## 5. Solver invariants and metamorphic tests

Use small enumerated fact domains to check transfer distributivity, singleton
representation, zero generation/propagation, and kill behavior. Compare supported
intraprocedural IFDS instances with an independent fixed-point evaluator. Validate
call summaries against explicit call/return-matched paths in finite examples.

Required metamorphic properties:

- Alpha-renaming a bound variable preserves normalized flows.
- Formatting/comments and adding irrelevant declarations preserve flows.
- Reordering independent statements preserves the relations they actually support.
- Reversing the before/after snapshots reverses additions/removals where alignment
  is unambiguous, and retains the same uncertainty reasons where appropriate.
- Identity comparison yields no flow deltas, even when both sides are partial;
  diagnostics are still present.
- Demand-sliced and unsliced analyses agree on supported query relations.
- A changed upstream initializer retains all unchanged downstream context; marking
  only the origin's file as edited must not exclude supported consumers elsewhere.
- Extending a query to an enclosing resolved caller context continues an escaping
  return through the matching return site, without adding paths from other callers.
- Cached and uncached results agree; changing a callee invalidates relevant summaries.
- Witness expansion cannot return through a different call site.

For performance runs, record hardware, repository revision, source file/LOC counts,
dependency/slice size, model versions, cold/warm state, p50/p95 timings and peak
process-tree memory. Exclude provisioning/download time, but include metadata
resolution. Validate the budgets proposed in [SPEC.md](SPEC.md) before committing
to tighter service-level targets.

## 6. Real-world acceptance case

[PR #5816](PR-5816.md) is the first agreed case. Select only `contentLangs` in
`loggedOutFetch`. Reduced early-stage fixtures retain the relevant shape but must
be labeled adaptations, not exact copies. Later stages use pinned full sources
and sufficient dependencies/models. This case must include forbidden flows to
the fallback request's language inputs and independent labeler data. Retain both
unchanged first-request language consumers. Require explicit modeled boundaries
at their external request inputs: a complete request-input slice does not prove
that the language change has no downstream server or application effect.

Future bugfix cases should vary branch complexity, overwritten values, call depth,
and uncertainty. Choose one seed binding per query. Existing PR #11683 and #11693
syntax fixtures can supply future integration material, but their passing tests
do not replace an independently reviewed data-flow oracle.
