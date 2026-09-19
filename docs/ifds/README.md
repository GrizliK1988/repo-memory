# IFDS analysis: specification and roadmap

Status: proposed implementation specification, agreed product scope. No IFDS
implementation or benchmark results are delivered by these documents.

The objective is to explain how a bugfix changes the definitions, uses, and paths
associated with one explicitly selected variable. The full repository is available
in both revisions. Results identify changed data flows and their evidence; they do
not claim to establish the runtime cause of a bug.

Each query includes the full supported upstream and downstream flow, not just the
path into the edited code. Follow derived copies and unchanged consumers until
their influence ends or the modeled execution exits. Scope, model, and unsupported
boundaries stay explicit; reaching one is not proof that later impact is absent.

Start with initialization, reassignment, and uses of local scalar bindings. Expand
to branches, loops, functions, and further TypeScript execution behavior in stages.
Keep the solver in a separate Rust `ifds` module with a language-independent core
and a TypeScript adapter. The existing `tsx_diff` module remains a syntax-change
analyzer and can help locate candidates.

Documents:

- [Implementation tasks](../../spec/kickoff/README.md): ordered Markdown tasks with
  dependencies, unit-test acceptance cases, and additional integration gates.
- [Analysis specification](SPEC.md): semantics, architecture, API, graph comparison,
  evidence, uncertainty, and resource limits.
- [Benchmark specification](BENCHMARKS.md): exact fixture rules plus the late-stage
  reviewed-suite precision, coverage, and release gates.
- [PR #5816 case study](PR-5816.md): the selected `contentLangs` variable and the
  expected before/after analysis at successive milestones.

## Implementation order

Stages are capability gates, not calendar estimates. A stage includes its adapter
rules, graph lowering, analysis rules, diagnostics, comparison behavior, and tests.
Unsupported relevant constructs must produce explicit diagnostics from stage 1.
The full repository may be indexed before the entire language can be analyzed.

| Stage | Implement | Required evidence before advancing |
| --- | --- | --- |
| 0. Contracts and test infrastructure | Snapshot/query contracts; binding, node, and edge IDs; source spans; full-slice graph/result schemas; directional coverage and path-ending records; unsupported-operation nodes; reviewed golden-fixture format; generic solver interface and tiny hand-authored IR examples. | Stable IDs under line shifts; malformed/ambiguous queries rejected; result serialization deterministic; transfer functions checked for distributivity; lifecycle endings distinguished from open boundaries; fixture expectations independent of the analyzer. |
| 1. Straight-line binding flow | One selected local `let`/`const` or parameter, analyzed from the beginning of its containing function with unknown input values; initializers, simple scalar reassignment, copies, literals, supported primitive expressions, template interpolation, reads, and ordinary return; reaching definitions and value provenance through the complete supported downstream chain; before/after delta. | Detect added/removed writes and replaced initializers; killed definitions do not reach later reads; copies retain their earlier origin after the original variable is overwritten; unchanged intermediate/final consumers remain in both graphs; shadowed bindings stay separate; unknown calls and values escaping the entry have explicit boundaries. |
| 2. Branches | `if`/`else`, nested conditionals, ternaries, `&&`/`||`/`??`, early returns, joins; explicit condition outcomes; constant-condition pruning and supported guard checks. | Correct definition sets on each branch; a branch can skip a write; a guard-only edit changes flow conditions; unreachable code after a return does not contribute; unresolved path feasibility remains explicit. |
| 3. Loops | `while`, `do/while`, classic `for`, loop-carried definitions, `break`/`continue`, nested loops; fixed-point solving and compact cyclic witnesses. | Zero-iteration and one-or-more-iteration cases differ correctly; `do/while` executes its body first; old and new loop-carried writers are compared; completion never depends on a fixed unrolling count. |
| 4. Functions and modules | Statically resolved direct calls; actual/formal mapping; return values and downstream caller consumers within an included entry context; multiple call sites; direct/mutual recursion; imports, exports and re-exports; TypeScript project resolution; call/return-matched IFDS tabulation and summaries. | No flow leaks between different callers; analysis follows argument/return chains into unchanged files and does not stop at a callee's exit; caller locals survive unrelated calls; reassigning a scalar parameter does not rewrite the caller's binding; recursion converges without implying runtime termination; out-of-scope receivers, unresolved targets and missing dependencies are reported. |
| 5. Read projections and summaries | Literal object/array construction and property/index reads; bounded read paths; string/array `join`, string `split`, and other individually audited summaries; call argument projections; closed, known object spreads. General heap writes remain deferred. | Distinguish sibling fields and URL/header inputs; unknown spreads/getters remain boundaries; modeled sources and library assumptions are visible. The main PR #5816 request-input slice becomes explainable with declared summaries. |
| 6. Exceptions and basic `await` | `throw`, `try`/`catch`/`finally`, normal and exceptional call exits; fulfilled/rejected continuations of explicitly awaited operations; promise result projections and normal-flow request/response boundaries. | A throwing RHS does not commit its assignment; `finally` runs with correct pending return/throw/break behavior; rejected awaits follow exceptional paths; PR #5816 is an end-to-end acceptance case within its declared source/sink boundaries. |
| 7. Mutation and aliases | Object/array writes; allocation-site abstractions; aliases; strong versus weak updates; destructuring writes; compound/update/logical assignments; mutation through arguments; bounded access paths and invalidation. | Separate binding reassignment from heap mutation; aliases expose possible writers; unknown alias sets do not permit strong kills; widening/access-path limits create explicit uncertainty. |
| 8. Additional synchronous TypeScript | `var` hoisting and uninitialized bindings; TDZ handling; destructuring/default/rest parameters; `switch` and fallthrough; labels; classes, constructors, `this`, inheritance, instance/static fields, getters/setters; closures and captured bindings; resolvable function values and callbacks. | Each syntax family has positive, negative, and flow-delta fixtures; closure capture and per-iteration bindings are modeled; unresolved dispatch is explicit; types do not masquerade as runtime writes. |
| 9. Feasibility and matching refinement | More guard reasoning; finite predicate abstractions or bounded witness validation; uncertain node alignment; resolved dispatch refinements; selected-variable inference as an optional convenience. | Contradictory paths are not emitted as supported flow claims; validation timeout becomes unknown; alternative witnesses are considered; matching improvements reduce spurious additions/removals without guessing identity. |
| 10. Deferred and framework execution | Promise chains/combinators; deferred callbacks, timers and event listeners; generators/iterators, `for...of`/`for await...of`; audited React state/effect and other framework summaries; module initialization/cycles and dynamic import where resolvable. | Registration is distinguished from callback execution; tracked values escaping into deferred work continue beyond the registering function where modeled; event ordering and mutable captures are explicit; unsupported scheduling and framework behavior are not flattened into synchronous flow or mistaken for lifecycle completion. |
| 11. Broader coverage and scale | Add syntax/library families according to benchmark misses; demand-driven refinement, cached summaries and invalidation; bounded output; parallel independent snapshot analysis. | Maintained precision and coverage gates, deterministic results, cold/warm performance measurements on a pinned medium-sized repository, explicit partial results under limits. |

Stages 9 and 11 are continuing refinement work, not permission to defer basic
semantic correctness or uncertainty reporting. Basic call/return precision is
required in stage 4 and is enforced by exact tests. Aggregate benchmark measurement
is deliberately deferred to stage 11, when a frozen reviewed real-repository suite
exists. Unknown behavior must never be silently treated as identity.
Limited read-only projection support in stage 5 does not imply mutation/alias support.

## First deliverable: stage 1

The first implementation accepts two repository snapshots and a variable selector,
lowers its enclosing function's supported straight-line code, computes definitions
and origins, and emits graph deltas with source locations and witnesses. Each
function parameter is a labeled input whose value is initially unknown. Unsupported
statements that can affect the query produce an unknown frontier and an incomplete
result for that region. Traverse beyond the changed line and first consumer, keeping
the full supported chain. Report proven endings separately from unknown/model/scope
boundaries; an exported return value has an open caller continuation at this stage.

It must distinguish these cases:

```typescript
// Before
let language = "en";
let requestLanguage = language;
language = "de";
return requestLanguage;

// After
let language = "fr";
let requestLanguage = language;
language = "de";
return requestLanguage;
```

The first initializer changed. Its value still flows through `requestLanguage` to
the return. The later write to `language` does not reach that copied value. The
copy and return can be impacted despite unchanged syntax. Selection targets the
`language` binding, including its writes, rather than every identifier with that
spelling. Complete source fixtures wrap this fragment in a function.

## Later use: consumer-history correlation

Preserve unchanged consumer identities, input projections, expressions, dependency
paths, snapshot metadata, and coverage boundaries so a later feature can compare
them with previous changes or bugfixes. A historical consumer-side issue may be
relevant to a current upstream edit even when that consumer is unchanged now.
This roadmap supplies the evidence, not automatic history mining or a proof that
a previous bug has recurred; those require a separate later implementation.

## Definition of done for each stage

- Newly supported semantics and supported/unsupported boundaries are documented.
- Every new syntax family has independently reviewed expected positive and negative
  flows, before/after deltas, and uncertainty diagnostics.
- Existing syntax-change tests remain passing. The IFDS test suite is separate.
- Promoted exact fixtures match all expected nodes, relations, and diagnostics.
- Full upstream/downstream context, unchanged consumers, and path endings match
  the oracle; a short prefix cannot pass as a complete value lifecycle.
- Before the late-stage benchmark gate exists, independently authored exact fixtures
  pass for every promoted supported case, including forbidden-flow assertions.
- At stage 11 and release, reported-flow precision is at least 90% on the frozen
  reviewed integration/held-out benchmark; recall, abstentions, and unsupported
  coverage are published alongside it.
- Analysis budgets, assumptions, and witness truncation are visible in output.
- No roadmap entry is described as implemented before its gate passes.
