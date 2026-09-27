# Function result analysis and comparison

Status: planned extension; no function-result API, report, or acceptance tests are
implemented by this document. Delivery tasks: [function results](../../spec/function-results/README.md).
This extends the [shared specification](SPEC.md) with a new analysis target.

## 1. Product contract

Given two immutable repository snapshots, their diff, and one selected function,
explain what the function returns, which conditions select each result, and how
those rules changed. Users do not need to select a local variable. Parameters,
local writes, intermediate expressions, and controlling decisions become relevant
through their influence on the selected function's result.

The primary observation is the whole normal return value. Include all reachable
explicit returns, early returns, expression-bodied arrow results, and implicit
`undefined` on reachable fallthrough. Compare both versions under the same inputs
and declared entry assumptions. A return expression may be described symbolically
when its exact value is unknown.

Required questions:

1. Under which conditions does each normal return occur?
2. What value or expression supplies that result, and through which dependencies?
3. Which input regions have proven different values or changed return computations?
4. Which conditions or computations changed while the result stayed provably equal?
5. Which answers remain unresolved, and what prevents a conclusion?

The first delivery covers the existing scalar expression and branch subset in a
single synchronous function. It adds bounded result comparison, not general
TypeScript equivalence checking. Side effects, exceptions, nontermination, heap
identity, async completion, and generator yields are separate observations or
later capabilities. Equal normal results do not establish equal overall behavior.
Relevant unsupported effects must still prevent unsupported return claims.

## 2. Query, inputs, and compatibility

Add an explicit function-result target alongside the existing binding target.
Conceptually, target selection can be shared internally:

```text
AnalysisTarget = Binding(binding_selector) | FunctionResult(function_selector)

analyze_function_result(query: FunctionResultQuery)
    -> Result<FunctionResultReport, InputError>
```

These are proposed contracts. Preserve `VariableFlowQuery`, `VariableFlowReport`,
`VariableSourceReport`, and their APIs and serialized meaning. A separately
versioned result report may reuse common evidence structures. Do not encode this
mode as a guessed local binding or rewrite user source to insert a result variable.

| Query concept | Required contract |
| --- | --- |
| Snapshots and diff | The existing immutable content, diff validation, rename, capability, model-version, and budget rules apply. |
| Function selector | One snapshot side, repository-relative path, and UTF-8 span identifying an exact ordinary function declaration, function expression, or arrow; optional name and owner assertions detect stale selectors. Names alone are insufficient. Class and object methods join this target when K031 is delivered. |
| Counterpart | Optional explicit function selector on the other side; otherwise require unique correspondence using supported file/path, name, and owner evidence. A renamed function needs an explicit counterpart in the first delivery. Automatic rename tracking is a later feature. Distinguish confirmed function addition/removal from ambiguous matching. |
| Observation | All normal results owned by that function. Returns inside nested function bodies belong to their own procedures. |
| Entry | The selected function's entry with labeled unknown positional inputs by default. Record explicit typed assumption expressions over those inputs and any known values, separately for each snapshot when they differ. A later call-context selector may supply actual arguments. |
| Input correspondence | For a common call context, pair positional actual arguments by index and map them to each version's formal parameters. This covers renames and reorderings of simple formals without matching their names. Added/removed parameters and default, rest, or destructured forms require supported argument semantics or explicit justified assumptions; otherwise affected comparisons remain unresolved. |
| Scope | Dependencies needed to explain the result within the declared capabilities and execution context. Following later caller consumers is an explicitly included extension. |

For each paired input, both versions receive the same value. External state can
only be coupled across revisions through an explicit common input or named model;
identical source text is insufficient. If entry assumptions differ, compare their
established common domain and disclose excluded regions. Do not silently replace
different inputs with a common name or fabricate a result on a missing function side.
An explicit mapping must state why two input slots receive the same runtime value;
it cannot turn a changed call signature into an unchanged call. A reordered
formal can receive a different argument even when its name remains unchanged.
At the default function entry, use symbolic positional argument slots; a resolved
call context supplies concrete or modeled values for those same slots.
Represent assumptions as structured expressions with typed operators and operand
references, not display strings. Retain both versions of an edited assumption,
their input dependencies, and their provenance as query scope. Expression nodes
cover positional input references, primitive literals, and the supported K006/K015
Boolean, arithmetic, and comparison forms. FR001 preserves those nodes and their
operand roles; FR002 reasons about them only where its declared theory establishes
feasibility. Other predicates remain visible with unresolved feasibility. An
assumption edit changes the query domain and is not a source-code change. FR002
compares established common domains and discloses one-sided or unresolved regions.

Boolean-only examples below declare Boolean entry domains. A TypeScript type
annotation alone is not proof that every runtime caller satisfies that domain.
The first function-result delivery selects ordinary declarations, function
expressions, and arrows. [K031](../../spec/kickoff/031-classes.md) extends this to
class and object methods with receiver-aware result observations. Automatic
correspondence for renamed functions belongs to the
[later rename-tracking feature](../../spec/function-rename-tracking/README.md).

## 3. Result observations and dependency construction

Analyze each snapshot independently using the existing lowering, forward solver,
and branch evidence. Then build a backward dependency slice rooted at every
reachable result observation. This requires a new slice criterion and assembly
path; it does not require reversing IFDS transfers or changing their distributivity.

Each observation retains:

- the owning procedure and snapshot-local return/exit identity;
- the source span and whether it is explicit return, bare return, arrow result,
  or fallthrough;
- its effective reachability condition, including negated earlier-exit conditions;
- the whole returned expression and its exact operand dependencies;
- immediate writers, upstream origins, controlling guards, and evidence references;
- unresolved contributors and applicable entry/model assumptions.

Follow reaching definitions through copies, computations, and value-producing
joins. Include conditions that choose a writer as well as conditions that enable
the return itself. Preserve assignment order, overwrites, short-circuit evaluation,
and the value version read by each guard. A guard selecting a constant return is
a control dependency, not a copied value origin.

Direct literal returns must work without any local binding or parameter. For
`return x + 4`, retain the addition as the whole result, with `x` and `4` in their
operand roles. Do not label a source of `x` as the value of the entire expression.
Exclude unreachable returns and overwritten values that cannot affect any result.
An earlier copy may still carry an overwritten origin into a result.

Bare `return;` and reachable ordinary fallthrough return `undefined`. This value
must be representable explicitly, distinct from missing analysis data, `null`,
throwing, or absence of a normal exit. An unknown call before a return may affect
whether it is reached even if its output is unused. A nested function declaration
does not execute that nested body. Do not classify unsupported paths as fallthrough.

The graph includes unchanged dependencies outside the diff. An unrelated sibling
use of an upstream input need not enter a function-result slice. Existing binding
queries continue to include their full upstream and downstream slices.

A complete result observation can end at the selected function's normal return.
Its returned value has an open caller continuation unless that context is included.
Record result-observation completeness separately from value-lifecycle closure.

## 4. Comparing conditions and results

### Shared input regions

Construct a guarded result summary for each snapshot, retaining shared expression
and condition structure. For a before alternative with guard `G_before` and an
after alternative with guard `G_after`, compare their results in the region:

```text
common_entry_assumptions AND G_before AND G_after
```

Establish region feasibility before making a supported comparison. Never compare
two mutually exclusive paths as if they occurred on the same inputs. Retain
unknown regions and uncovered entry inputs explicitly. The report need not list
every path pair: share guards, expressions, and equal facts and group regions only
when equivalence is established. Budgets may yield partial comparisons.
These guards are symbolic. A supported Boolean rewrite such as `flag` to `!flag`
can establish a changed return selection without evaluating `flag` for a
concrete input or invoking a general predicate solver.

Return-site alignment and result equivalence are separate evidence dimensions.
An early return can be split, merged, or moved while preserving the same result
rule. Preserve uncertain site correspondence; complete summaries can still prove
result equality if the function/input correspondence and semantic proof are
independent of that ambiguity. Source attribution stays unresolved in that case.

### Result relations and evidence

Each compared region has a result assessment and a separate evidence category:

| Result assessment | Required meaning |
| --- | --- |
| `equal` | Both versions normally return equal values for every paired input in the stated feasible region, under the recorded assumptions. |
| `different` | Both versions normally return unequal values for every paired input in the stated feasible region, under the recorded assumptions. |
| `changed` | Both normal results are established in the region and the returned expression, its established value dependencies, or its selection rule changed. Their concrete values need not be computed or proved unequal. |
| `unknown` | The available evidence cannot establish either value equality/inequality or a changed result computation/selection for the region. Include the reason. |

Use the shared `supported`, `modeled`, and `unresolved` evidence categories. An
`equal` or `different` result relying on a named model must identify that model.
An example input is an optional witness, not proof about a larger region. If only
one concrete input is established, restrict the claim to that input. Do not claim
all inputs change from a possibly differing expression.

Classify a feasible region in this order: proven unequal values give `different`;
proven equal values give `equal`; otherwise an established change to the
reachable return computation or source-selection rule gives `changed`. Use
`unknown` when reachability, input pairing, dependencies, or matching prevents
even that structural conclusion. A separate finding still records a changed
condition or expression when the value relation is `equal` or `different`.
Do not evaluate an expression merely to produce `changed`. An edit overwritten
before every return is not a changed result computation.

For example, `return p + 1` changing to `return p + 2` is `changed` on the
supported normal-return region. The report shows both expressions and the edited
operand. It does not claim that the numerical values differ for every input.
Likewise, returning paired input `p` before and independent paired input `q`
after is `changed` when their distinct dependencies are established, even
though the inputs may happen to hold equal values.

A guard edit must also show its effect at the return. For `if (flag) return "yes";
return "no";` changing to `if (!flag) return "yes"; return "no";`, report
the changed condition, the swapped return-selection rules, and the affected
returned values. Under an explicit Boolean input domain, symbolic polarity and
the two distinct literals establish `"yes" -> "no"` when `flag` is true and
`"no" -> "yes"` when it is false. A simple changed-selection finding is
available without evaluating the guard. Do not report only a changed condition
while leaving its supported return consequence unexplained.

The first comparison theory must support Boolean input conditions, negation,
supported conjunction/disjunction and branch composition, primitive literal
equality/inequality, identity of a paired unchanged primitive input or copy,
and structural identity of the same supported pure deterministic expression
over paired primitive inputs. This last rule establishes equality without
evaluating the expression; unknown coercions or effects invalidate it.
Evaluate only the supported operations with defined language semantics. Primitive
result equality is an observational comparison of type and value, not JavaScript
strict equality. If special numeric values are modeled, use SameValue semantics:
NaN equals NaN and positive and negative zero differ. The first delivery need not
model those special values; until it does, their relations are unknown. Unsupported
numeric reasoning, coercions, object identity, or expression equivalence stays
unknown. Broader predicate theories remain owned by K032.

Keep condition identities tied to binding/value versions and caller context.
Reason over typed conditions and expressions, not display-string substitutions.
Keep this relational comparison layer separate from the IFDS fact transfers.

Equal source sets do not prove equal results. Changed source sets, operators, or
conditions do not prove unequal values; established changes affecting the returned
computation can support `changed`. Report condition/computation findings
independently of the assessment. If both branches return `1`, changing which
branch executes can retain `equal` results while changing control evidence.

A whole-query "normal results unchanged" claim requires complete coverage of the
common declared input domain with `equal` regions and established normal completion
on both sides. Unknown paths, possible unsupported throws, termination uncertainty,
or comparison limits prevent that claim. Independent complete regions remain
reportable. Future return-versus-throw or return-versus-divergence findings require
an explicit completion comparison; neither is an `undefined` value comparison.

## 5. Report and presentation

Provide text, compact JSON, and full evidence modes. The conceptual
`FunctionResultReport` has a separately versioned schema with:

| Content | Required facts |
| --- | --- |
| Identity and scope | Snapshot pair, selected function and counterpart, target kind, input mapping, typed before/after entry-assumption expressions and their common/one-sided domains, versions, and limits. |
| Observations | All relevant return sites and normal exits, with conditions, whole expressions, locations, and evidence. |
| Dependencies | Shared sources, computations, operand roles, controls, and upstream links. |
| Result regions | Same-input conditions, before/after results, assessment, evidence category, and proof or unknown-reason references. |
| Findings | Changed conditions, computations, result selection, observations, or function presence, linked to affected regions and supported edit evidence. |
| Coverage | Per-snapshot analysis, source alignment, input mapping, result comparison, presentation completeness, and open caller continuations. |

Each finding answers: what changed, under which inputs, what was returned before
and after, and how the dependency evidence connects the edit to the result. Link
all established contributing edits; do not guess a unique cause. Changes to
control or expression structure may be reported with an equal or changed result.
For a guard edit that changes which return is selected, show the guard before and
after together with the affected return choices. A condition-only sentence is
insufficient when its return consequence is established.
For each affected return choice, show its complete selection condition before
and after the edit, including unchanged enclosing guards that constrain when
the choice is reachable. Then show the same-input region where the selected
result changes. In compact text and JSON, summarize other return choices as
unchanged when their conditions and results are proven unchanged; retain their
details in full evidence. If coverage is partial, do not summarize unexamined
choices as unchanged.

For compact presentation, factor a proven common condition out of the affected
selection rules and display it once as an explicit context. Present a table of
return choices with before/after conditions relative to that context, followed
by one concise result-change statement. Every relative condition, including the
effect region, means the context AND that condition. Factoring must preserve
the complete selection rules; do not omit guards that differ across versions.
Compact JSON must represent the context and its association with the relative
conditions structurally. Avoid a separate condition-edit sentence when the
table already conveys the same fact. Source locations and derivations may be
available through detail references, but unknown results and incomplete coverage
must remain visible in the compact view.

Human text and compact JSON derive from the same structured facts. Full evidence
contains both dependency graphs, comparison derivations, and diagnostics. Compact
references resolve into that exact report and snapshot pair. Reuse the compact
report's deterministic IDs, sharing, and evidence principles without requiring a
selected binding or silently changing `VariableSourceReport`.

Presentation size should follow distinct result rules and findings. Count omitted
groups and provide detail references when output is truncated. Keep analysis,
comparison, and presentation limits distinct; an empty partial delta list must not
render as "behavior unchanged".

## 6. Required examples

### A condition changes which result is returned

Under explicit Boolean entry assumptions for `enabled` and `ready`:

```typescript
// Before
function result(enabled: boolean, ready: boolean) {
  if (enabled) return "ok";
  return "skip";
}

// After
function result(enabled: boolean, ready: boolean) {
  if (enabled && ready) return "ok";
  return "skip";
}
```

| Shared input condition | Before | After | Result relation |
| --- | --- | --- | --- |
| `enabled && ready` | `"ok"` | `"ok"` | `equal` |
| `enabled && !ready` | `"ok"` | `"skip"` | `different` |
| `!enabled` | `"skip"` | `"skip"` | `equal` |

Required finding: adding the `ready` check changes the normal result from `"ok"`
to `"skip"` under `enabled && !ready`. The set of possible results is unchanged.
Retain both return sites and the shared control evidence without duplicate findings.

### Report affected return choices with their selection conditions

Under explicit Boolean entry assumptions for all four parameters:

```typescript
// Before
function status(enabled: boolean, blocked: boolean,
                ready: boolean, approved: boolean) {
  if (!enabled) return "disabled";
  if (blocked) return "blocked";
  if (ready) return "ok";
  return "pending";
}

// After
function status(enabled: boolean, blocked: boolean,
                ready: boolean, approved: boolean) {
  if (!enabled) return "disabled";
  if (blocked) return "blocked";
  if (ready && approved) return "ok";
  return "pending";
}
```

A compact report displays the common context `enabled && !blocked` once.
All conditions below are relative to that context:

| Return choice | Before selection | After selection |
| --- | --- | --- |
| `"ok"` | `ready` | `ready && approved` |
| `"pending"` | `!ready` | `!ready \|\| !approved` |

Effect within that context: when `ready && !approved`, `"ok" -> "pending"`,
assessed `different`. The full changed-result region is therefore
`enabled && !blocked && ready && !approved`. The `"disabled"` and `"blocked"`
conditions and results are summarized as unchanged. Full evidence still records
their return sites and coverage. The compact view need not repeat the condition
edit already visible in the table.

### Additional required cases

Snippets are synchronous function bodies with declared primitive/Boolean input
domains as appropriate. Expected results are authored independently of the analyzer.

| Case | Before -> after | Required conclusion |
| --- | --- | --- |
| No selected variable | `return 1;` -> `return 2;` | Unconditional `different`; no binding selector is needed. |
| Equal branch results | `if (flag) return 1; return 1;` -> `if (!flag) return 1; return 1;` | Changed control; unconditional equal normal result, even if individual return-site alignment is unresolved. |
| New early return | `return 2;` -> `if (flag) return 1; return 2;` | `different` under `flag`, `equal` under `!flag`; retain the final-return continuation guard. |
| Implicit result | `if (flag) return 1;` -> `if (flag) return 1; return 2;` | `undefined -> 2` under `!flag`; the `flag` region remains equal. |
| Equivalent return shape | `return flag ? 1 : 2;` -> `if (flag) return 1; return 2;` | Equal results throughout the Boolean input domain despite changed observation structure. |
| Overwritten source | `let x = p; x = 3; return x;` -> replace `p` with `q` | Result remains 3; the killed initializer is not a reaching result source. |
| Copy survives overwrite | `let x = 1; const y = x; x = 3; return y;` -> initializer 1 becomes 2 | Result changes 1 -> 2 through `y`. |
| Same result input | `return p;` -> `const copy = p; return copy;` | Equal for the same paired primitive input, with changed intermediate structure. |
| Computation changes | `return p + 1;` -> `return p + 2;` | Report `changed` with both whole expressions and the edited operand; no numeric evaluation or claim of unequal values is required. |
| Guard inversion selects a different return | `if (flag) return "yes"; return "no";` -> `if (!flag) return "yes"; return "no";` | Show `flag -> !flag` and the resulting switch of return values for the same symbolic Boolean input, without running the condition. |
| Guard version changes | `let g = flag; if (g) return 1; return 2;` -> insert `g = !g` before the condition | Track the new value of `g`; result selection reverses for Boolean `flag`. |
| Unknown completion | `return 1;` -> `mystery(); return 1;` | The unknown call prevents an unconditional equal-completion claim. |
| Unsupported return | `return p;` -> `return mystery(p);` | Known argument dependency does not establish the call result; comparison is unknown. |
| Nested function | Edit a return inside an uncalled nested function; outer function still returns 1 | Nested returns are not outer observations. |

Also require stale selectors, ambiguous function/input matching, confirmed function
addition/removal, unreachable returns, reordered conditional overwrites, and
analysis/comparison/presentation budget fixtures. A known changed region must
survive an independent unknown region without certifying the unknown region.

## 7. Extension boundaries

| Extension | Required semantics and existing owner |
| --- | --- |
| Loops | K016 supplies loop entry, zero iterations, carried definitions, `break`/`continue`, fixed points, and compact cycles. Result summaries add loop-exit conditions and contributing updates. Fixed unrolling is not a complete algorithm; convergence does not prove termination or exact accumulated values. |
| Resolved calls | K017/K018 supply target resolution, actual/formal mapping, and matched returns. Compose guarded callee result summaries in each call context; preserve relevant unchanged callees outside the diff and keep two call sites distinct. |
| Recursion and caller continuations | K019/K020 supply finite recursive summaries and explicit enclosing caller context. Propagate a changed result to later caller consumers only within included scope. Recursion convergence does not prove runtime completion. |
| Exceptions and async results | K023/K024 and later scheduling work own completion semantics. A future observation may distinguish return, throw, fulfillment, and rejection; `finally` can replace a pending return. |
| Broader value/guard reasoning | K032 adds declared predicate theories and bounded validation. Timeouts remain unknown. Heap/field observations also require the corresponding projection and alias capabilities. |
| Class and object methods | K031 adds exact method procedure identities, receiver-aware inputs, and result selection. Unresolved dispatch or receiver effects retain an unknown boundary. |
| Renamed functions | The post-kickoff FRT001 task adds automatic counterpart matching when independent evidence yields one unique function. The first delivery requires an explicit counterpart for a rename. |

These extensions require result-specific acceptance cases when enabled: zero versus
one-or-more iterations, changed loop exit, two calls without result leakage,
callee-only edits, recursive uncertainty, and scoped caller impact. They are not
prerequisites for the first single-function delivery. A static call context is not
a captured runtime stack; selecting an actual execution requires separately
supplied trace information and a future query contract.

## 8. Acceptance

Implement [FR001–FR003](../../spec/function-results/README.md) in dependency order.
Use exact positive, equal-result, changed-result, and uncertainty fixtures with
independent expectations, including all examples above. Preserve all existing
variable-flow tests and output contracts. Run the shared offline implementation
gates when code is delivered; adding this document does not satisfy those gates.

Score result assessments separately from flow-edge deltas when aggregate benchmarks
are introduced. Correct dependency edges alone do not establish correct behavioral
claims. Publish unknown regions and coverage alongside changed/equal claims, and
never treat abstention as a proof of unchanged behavior.
