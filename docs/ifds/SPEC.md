# Variable flow analysis specification

Status: design for a future separate Rust module. The API records below are
contract sketches, not code that currently exists.

## 1. Product contract

Given before/after repository snapshots, their Git diff, and one selected binding,
explain which nodes write it, which definitions reach reads, how its value reaches
other expressions, and what changed between revisions. Include sources feeding
the binding, downstream uses of its values, branch conditions, and relevant call
boundaries. A read of another binding becomes relevant through an actual dependency,
not just because it appears in the same function.

The query covers both upstream origins and downstream impact. The declaration or
edited operation is an anchor, not the end of the analysis: follow the selected
binding's values through all supported later consumers, including unchanged code
outside the diff. Continue each path until its value/control influence ends or
the modeled execution ends, whichever happens first. An unsupported operation or
scope boundary is an explicitly open continuation, not proof that the flow ends.
Section 5 defines these stopping rules, including copies that outlive the binding.

By default, analysis starts at the beginning of the function that contains the
selected variable. It asks: "If this function is called, which assignments and
uses of this variable are possible?" The analyzer reads the code without executing
it. It initially does not investigate which application code calls this function.
This default limits caller context, not traversal to later statements or known
callees. A value returned out of this entry function has an explicit caller-scope
boundary until an enclosing caller context is included; do not label it the end
of the value's lifecycle or of the application execution.

Each parameter starts as a labeled input whose value is unknown to the analyzer.
For example, in the following function, `preferred` is an input source and
`useFallback` determines whether the second assignment happens:

```typescript
function chooseLanguage(preferred: string, useFallback: boolean) {
  let language = preferred; // Selected variable.
  if (useFallback) {
    language = "en";
  }
  return language;
}
```

Without information about the caller, the analysis must consider both outcomes
of `useFallback`. Its report says the returned `language` may come from `preferred`
or from the fallback assignment. Branch support for this example arrives in
stage 2; the starting-point rule also applies to stage 1's straight-line code.

Later, a query can start from a specific call, such as
`chooseLanguage("de", false)`. That supplies known arguments: the fallback branch
is then excluded, and the returned value comes from the first assignment. This
additional information about a call is what "caller context" means here.
When that call's enclosing caller context is included, downstream analysis follows
the returned value into the caller's later statements as well.

The report records where analysis started and what was known about its inputs.
Starting inside a function does not establish that the application actually calls
it or that any particular path ran when the reported bug occurred.

The first query selects a scalar local binding or parameter. Initializers and all
subsequent writes belong to the query. Copying its value into another binding does
not make those bindings aliases. Mutation of referenced objects is a later capability.
The initial report is machine-readable, with concise evidence suitable for a future
human explanation. Preserve unchanged downstream consumers so later work can
correlate an upstream change with earlier fixes or changes at those consumers.
Historical correlation, a UI, and automated bug-cause conclusions are outside the
initial implementation; a current flow connection alone does not prove a past bug
is present or that the edited origin caused it.

## 2. Architecture and separation

Proposed module structure, created incrementally:

```text
src/ifds/
  mod.rs                    public analysis entry point
  model.rs                  IDs, facts, queries, reports, diagnostics
  ir.rs                     language-independent operations and control-flow graph
  solver.rs                 worklist, path edges, call/return summaries
  reaching.rs               last-write transfer rules
  provenance.rs             origin propagation and evidence
  compare.rs                snapshot alignment and flow deltas
  adapters/typescript/      parsing, binding, lowering, calls, audited summaries
tests/ifds/                 independent semantic/solver/regression tests
```

The core must not import Tree-sitter node kinds or JSX-specific types. The adapter
maps syntax and project metadata into IR and finite flow functions. Reuse
`changes::ChangeKind` where appropriate, but keep graph nodes, flow deltas, and
uncertainty records distinct from the current `ChildChange` API. Neither
`tsx_diff::SymbolKind` nor a qualified-name/source-order match is sufficient as a
semantic binding identity.

Tree-sitter handles syntax; select the TypeScript grammar for `.ts` and the TSX
grammar for `.tsx`. Stage 1 implements lexical binding for its restricted subset.
Before stage 4, introduce a versioned TypeScript metadata provider for project
resolution and declaration identities. The intended provider is a small Node-based
bridge around the TypeScript compiler API, communicating through deterministic
JSON. The Rust solver stays independent of that provider. Pin the compiler version
and record it in reports; unavailable metadata produces diagnostics, not guessed
call edges. Compiler symbols and signatures assist resolution but do not prove
runtime dispatch or external function effects. The official API exposes programs,
symbols and type information. [TypeScript compiler API](https://github.com/microsoft/TypeScript/wiki/Using-the-Compiler-API)

Respect the snapshot's `tsconfig`, path aliases, imports/re-exports, file extensions,
and declared module-resolution mode. A call target must be consistent with the
runtime/module configuration; declarations from `.d.ts` provide type information,
not executable bodies or purity guarantees.
[TypeScript module resolution](https://www.typescriptlang.org/docs/handbook/modules/theory.html)

Index the supplied repository, excluding vendored/generated files according to
explicit configuration. Analyze relevant procedures and dependencies. An excluded
or missing dependency on the query path is an explicit boundary. Reading source
and compiler metadata must not require running the application, tests, package
scripts, custom compiler plugins, or contacting external services.

## 3. Query and identities

The planned entry point is conceptually:

```text
analyze_variable_flow(query: VariableFlowQuery) -> Result<VariableFlowReport, InputError>
```

| Query field | Contract |
| --- | --- |
| `before`, `after` | Read-only repository snapshot handles with revision/content IDs. A Git provider reads immutable revisions without modifying the working tree. |
| `diff` | Repository diff matching those snapshots; may be derived by the provider. Validate changed-file contents and rename metadata before analysis. |
| `selected_binding` | Exactly one binding, identified by snapshot side, repository-relative path, and declaration identifier span in UTF-8 bytes. Optional enclosing-symbol/name assertions detect stale selectors. |
| `counterpart` | Optional explicit declaration span on the other side. Otherwise align uniquely; a newly added or removed binding may have no counterpart. Ambiguity is reported. |
| `entry` | Where analysis starts and what it knows about the inputs. Default: the beginning of the function containing the selected variable, with labeled but unknown parameter values. Later: a specific call, its known arguments, and an explicitly included enclosing caller context for following returned values into later consumers. Record the outermost included execution context. |
| `capabilities`, `summaries` | Supported-language stage and versioned model set; identical in both analyses. |
| `limits` | Time, memory, processed path-edge and output/witness budgets. |

Full upstream/downstream traversal within the declared entry context and models
is the default, not an optional diff-only mode. Report that scope and every open
continuation on both sides. The diff selects change evidence; it must not restrict
analysis to changed files, changed nodes, or the first downstream consumer.

Reject selectors that refer only to a reference occurrence, multiple bindings, a
different snapshot hash, or an unsupported initial target kind. Line/column input
may be a convenience wrapper with an explicit coordinate convention. Output spans
contain UTF-8 byte ranges and one-based inclusive lines; names alone are not IDs.

`BindingId` identifies lexical declarations, separating shadowed names. `NodeId`
and `DefinitionId` identify static operations inside one snapshot. Calls have
`CallSiteId`s and independent return sites. Comparison uses a separate
`LogicalNodeId` mapping; IDs from different snapshots are not directly equated.
Loop iterations and recursive activations do not allocate unbounded static IDs.

## 4. Control flow and facts

Use a forward may analysis: a reaching definition can supply a read on at least one
admissible path. This does not mean the write occurs on every path or changes the
concrete value on every execution. For example, `x = x` is a write even if its value
is preserved. A later optional value-equivalence analysis must use separate claims.

IFDS requires a finite fact domain and transfer functions distributive over the
chosen merge operator. Its solver respects matched call/return paths; this alone
does not establish that every branch combination is executable. Semantic accuracy
also depends on the language model. These are the relevant constraints from the
original framework. [Reps, Horwitz and Sagiv, POPL 1995](https://doi.org/10.1145/199448.199462)

### IR operations

Operations include `Entry`, `Read`, `Write`, `Compute`, `Branch`, `Join`, `Call`,
`ReturnSite`, `Return`, `Exit`, and `UnknownEffect`. Later stages add `HeapRead`,
`HeapWrite`, exceptional exits, suspension/resume, and callback registration/entry.
CFG edges distinguish normal flow, branch outcomes, loop backedges, call, return,
call bypass, exceptional flow, and later deferred execution. Edges retain the
source construct, outcome and modeling assumptions.

Lower expressions into ordered operations and temporary places. RHS reads and
effects precede assignment commit; arguments are evaluated in language order.
Short-circuit branches must not evaluate their skipped operand. Separate call and
return-site nodes are required.

A **join** is the point where alternative paths meet and continue with the same
code. For example:

```typescript
let x = 0;   // Assignment A.
if (flag) {
  x = 1;     // Assignment B, only on the true branch.
}
return x;    // Both branches meet before this read.
```

On the true branch, B is the last assignment to `x`. On the false branch, it is A.
At the join, the analyzer keeps both possibilities: `{A, B}`. This combines
information about the two paths; it does not execute another assignment to `x`.
The report therefore identifies A or B as the writer that can supply the read,
without inventing a third writer at the join. Guard outcomes remain attached to
the evidence for each possibility.

### Two complementary fact families

The analyzer tracks two kinds of information, called **facts**:

- `LastWrite(x, A)`: assignment A may be the most recent write to `x` at this point.
- `Origin(x, source)`: the current value of `x` may depend on this earlier write
  or input source, possibly through several copies and calculations.

`LastWrite` answers "Who assigned this variable?" `Origin` answers "Where did the
value come from?" Both store sets of possibilities because different paths may
produce different answers. They describe dependencies, not the concrete runtime
values of variables.

The records can be described in pseudocode:

```text
Place = a variable binding or a temporary used to evaluate an expression
WriteSite = the ID of a write operation in the code
Source = a WriteSite, a function input, or a modeled external input

Fact = one of:
    LastWrite(place, writeSite)
    Origin(place, source)
    Zero

factsAtNode = set of Fact

# At a join, keep the possibilities from every incoming path.
function merge(incomingFactSets):
    return union of incomingFactSets
```

IDs refer to a finite collection of code locations, bindings, temporaries, and
input sources in a snapshot. Visiting a write again in a loop or recursive call
reuses its ID. This keeps the set of possible facts finite, as IFDS requires.

`Zero` is an internal marker carried along reachable control-flow paths. It allows
a write to introduce new facts even if its value has no previous variable source,
as in `x = 10`. It is unrelated to the JavaScript number `0` or to an uninitialized
variable. A node with no incoming facts cannot generate facts on its own.

For the initial analysis starting at a function's beginning:

```text
entryFacts = {Zero}
for each parameter p of the selected variable's containing function:
    entryFacts.add(LastWrite(p, parameterEntrySite(p)))
    entryFacts.add(Origin(p, parameterEntrySite(p)))
    entryFacts.add(Origin(p, FunctionInput(p)))
```

These input sources describe the query's starting assumptions. Once calls are
analyzed in stage 4, a known callee receives origins from its actual arguments;
do not introduce unrelated unknown inputs for every such call.

The following pseudocode handles a supported write after its right-hand side has
been evaluated. Earlier reads are captured in temporaries before later expression
effects, so `sourcePlaces` contains the already-evaluated inputs to this operation.

```text
function applyWrite(target, sourcePlaces, writeSite, incomingFacts):
    outgoingFacts = empty set

    for each fact in incomingFacts:
        if fact == Zero:
            # This reachable operation becomes the new writer of target.
            outgoingFacts.add(Zero)
            outgoingFacts.add(LastWrite(target, writeSite))
            outgoingFacts.add(Origin(target, writeSite))
            continue

        if fact.place != target:
            # Writing target leaves information about other places intact.
            outgoingFacts.add(fact)
        # Existing target facts are otherwise dropped: this write replaces them.

        if fact is Origin(place, source) and place is in sourcePlaces:
            # Carry the already-evaluated input's history into the new value.
            outgoingFacts.add(Origin(target, source))

    return outgoingFacts

# Examples of lowered operations:
applyWrite(temp, [], literalSite, facts)            # temp = literal
applyWrite(x, [temp], assignmentSite, facts)        # x = evaluated RHS
applyWrite(temp, [aTemp, bTemp], computeSite, facts) # temp = aTemp + bTemp
```

For every input of a computation, its origins are carried independently into the
result. The analyzer also records which RHS reads feed the new write and retains
solver evidence for those dependencies. Evidence bookkeeping is separate from
the fact sets shown above.

A worked example shows why both fact families are needed:

```typescript
let x = 10; // A
let y = x;  // B
x = 20;     // C
return y;   // D: read y
```

The table shows source-level write IDs; expression temporaries are omitted:

| After operation | Last writes to `x` | Origins of `x` | Last writes to `y` | Origins of `y` |
| --- | --- | --- | --- | --- |
| A | `{A}` | `{A}` | — | — |
| B | `{A}` | `{A}` | `{B}` | `{A, B}` |
| C | `{C}` | `{C}` | `{B}` | `{A, B}` |

At D, B is the last write to `y`, while A is an earlier source of its value.
Changing A can affect the returned value. C overwrites `x`, but does not overwrite
the value already copied into `y`. For `x = x + 1`, the old `x` is read before
the new write, so its origins are carried into the new value of `x` as well.

Each incoming fact is handled independently and the results are combined with set
union. This is the distributivity requirement that makes these rules suitable for
IFDS. Preserve and test that property when adding rules: processing two input fact
sets together must produce the same result as processing them separately and
combining their outputs. No rule may require two different facts to be present
together before it produces an output fact.

This pseudocode covers supported local writes and computations. Whole-set value
reasoning, alias discovery, and predicate solving need separate abstractions and
documented interfaces. At the heap stage, alias information used to decide whether
a write definitely replaces an old value must stay fixed during one solver run;
changing that information requires invalidating the affected results.

### Procedures and solver

A procedure here means a function. The solver moves facts through that function's
operations until it has found no new facts to pass on. It analyzes code without
running the function. Stages 1–3 handle one function at a time; stage 4 also follows
calls into other functions and brings their results back to the correct caller.

For one function, the basic worklist algorithm is:

```text
factsAt = empty set for every node
factsAt[entry] = entryFacts
worklist = [entry]

while worklist is not empty:
    if analysis budget is exhausted:
        return partial result with an explicit limit diagnostic

    node = worklist.removeOne()
    for each outgoing control-flow edge from node:
        # Apply this edge's supported rule: assignment, branch, return, etc.
        # Unknown behavior must use an explicit boundary rule and diagnostic.
        outgoingFacts = transfer(edge, factsAt[node])
        newFacts = outgoingFacts - factsAt[edge.destination]

        if newFacts is not empty:
            factsAt[edge.destination].addAll(newFacts)
            worklist.addIfAbsent(edge.destination)

return solved facts and their supporting evidence
```

A loop may send new facts back to a node already visited. That node is processed
again only when its input gains facts. Once an iteration adds nothing new, there
is no more work to schedule for those facts. The fact set is finite, so this process
does not need to unroll a loop a fixed number of times. Solved facts can still have
unresolved path conditions; the checks in section 7 determine what may be reported.

Function calls require additional bookkeeping. The calling function is the
**caller**; the function being called is the **callee**. For example:

```typescript
function decorate(value: string) {
  return value + "!";
}

let language = "en";
let first = decorate(language); // Call A.
let second = decorate("de");    // Call B.
```

At A, the origins of `language` enter the parameter `value`. The return expression
uses `value`, so those origins reach `first`. B has its own argument: the origins
of `language` must not reach `second` through A's invocation of `decorate`.

```text
Valid:   language -> A's argument -> value -> A's return -> first
Invalid: language -> A's argument -> value -> B's return -> second
```

To avoid analyzing the same function effects repeatedly, keep a **summary**: a
record of which exit facts follow from a particular function-entry fact. A summary
describes data dependencies, not a cached runtime return value. Apply it separately
at each call that supplied the corresponding entry fact. The stage 4 IFDS solver
uses the following scheduling pattern; helper names describe required behavior,
not an implemented API:

```text
onKnownCall(callSite, callerEntryFact, callerFact):
    # Retain only caller facts the call cannot change. This is "call bypass".
    scheduleUnaffectedCallerFacts(callSite, callerEntryFact, callerFact)

    for each calleeEntryFact produced by mapArgumentFact(callSite, callerFact):
        key = (callSite.callee, calleeEntryFact)
        waiter = (callSite, callerEntryFact, callerFact)
        waitingCallers[key].add(waiter)

        # Reuse work already started, including for recursive calls.
        scheduleCalleeEntryIfUnseen(key)
        for each exitFact already in summaries[key]:
            resumeMatchingCaller(waiter, exitFact)

onNewSummary(callee, entryFact, exitFact):
    key = (callee, entryFact)
    if summaries[key] already contains exitFact:
        return

    summaries[key].add(exitFact)
    for each waiter in waitingCallers[key]:
        resumeMatchingCaller(waiter, exitFact)

resumeMatchingCaller(waiter, exitFact):
    returnedFacts = mapReturnFact(waiter.callSite, waiter.callerFact, exitFact)
    scheduleFacts(waiter.callerEntryFact,
                  waiter.callSite.returnSite,
                  returnedFacts)
```

Argument mapping includes `Zero` propagation and parameter-entry write facts,
as well as copying argument origins into parameter places. Return mapping brings
the return value's origins into that call's result temporary. Local variables
inside the callee do not become variables of the caller. The subsequent assignment
of the call result uses the normal write rule, replacing the target's old value.

For example, `replace(language)` may reassign its own string parameter and return
that new value, but it cannot reassign the caller's `language` binding just because
that binding was passed as an argument. Writes through captured variables or
object references require the later closure and mutation models.

Recursive calls subscribe to summaries that may still be growing. Whenever a new
exit fact is discovered, the waiting calls are revisited. The implementation must
deduplicate entry-to-node fact records, waiting-call records, and summary pairs.
Their finite set allows the process to settle without creating a new copy of the
callee for every possible recursion depth. Explicit stacks and complete path
strings must not become part of the finite fact domain.

If the callee or its effects cannot be resolved, keep only caller facts known to
be unaffected and report uncertainty for the rest. An unknown call cannot simply
be skipped or treated as harmless.

Keep the call IDs and the summary evidence when constructing report paths too.
Combining all calls into a plain graph and following any available return edge
could recreate the invalid A-to-B path above, even if the solver found correct
facts. The one-function worklist alone is therefore insufficient for stage 4.

## 5. What constitutes a flow

A flow is a connection that explains where a value came from, where it is used,
or which decision allows an operation to happen. Each connection has a specific
meaning, so the report can distinguish these questions:

| Relation | Question it answers | Example |
| --- | --- | --- |
| `reaches` | Which assignment can supply this read, before another assignment replaces its value? | In `x = 1; y = x`, the write `x = 1` reaches the read of `x`. |
| `value_dependency` | Which earlier value contributes to this expression or assignment? | In `y = x + 1`, the new value of `y` depends on the value read from `x`. |
| `controls` | Which condition determines whether this operation runs? | In `if (ready) { x = 1; }`, the true outcome of `ready` allows the write. |
| `argument` / `return` | How does a value enter a called function or come back from it? | In `y = decorate(x)`, follow `x` into the parameter and the return value into `y`, using the same call site. |
| `may_write` | Which operation can assign the selected variable or, in a later stage, mutate its tracked property? | In `if (ready) { x = 1; }`, the assignment may write `x`, with `ready` as its condition. Unknown effects retain their uncertainty. |

The distinction matters when a variable affects a decision:

```typescript
let x = input;
let y = 0;
if (x > 0) {
  y = 1;
}
return y;
```

`x` is read by the condition. That condition controls whether `y = 1` happens.
The assignment writes `y` and gets its assigned value from the literal `1`.
The report should show the control influence of `x`, without describing `y = 1`
as a write to `x` or a copy of `x`'s value. These possible connections still need
the path evidence and uncertainty checks defined in section 7.

For the selected variable, show the assignments that can write it, the earlier
sources feeding those assignments, and the later reads or operations using its
values. Include the conditions and calls needed to explain those connections.
An unrelated assignment does not belong in the report solely because it is nearby.
This relevant part of the program is called the selected variable's **slice**.

### Full upstream and downstream coverage

Build both parts of the slice for each tracked write, then retain their union:

- **Upstream:** definitions, inputs, and expressions feeding the write, plus the
  guards and call contexts needed to justify those connections.
- **Downstream:** every supported read, copy, calculation, argument, return, and
  consumer input reached by that write's value. Continue through intermediate
  consumers rather than stopping at the first use. Include unchanged consumers
  in other files when resolved calls carry the value there.
- **Control consequences:** if a tracked value feeds a guard, include the
  operations it controls and their supported downstream consequences, keeping
  the control step visible. For the `if (x > 0)` example above, the path into
  `return y` is through the decision controlling `y = 1`; it is not a value copy
  from `x` into the literal `1`.

This describes traversal of the facts and typed relations, not a requirement for
a second backward IFDS solver. Follow only justified dependencies and control
relations; do not include all code that happens to execute later. In particular,
an unrelated use of another input to a shared calculation is not downstream of
the selected variable merely because that input also appears in the upstream slice.

Track the lifetime of each written value and its derived copies, not only the
lexical lifetime of the original binding. Overwriting `x` kills the previous
definition in `x`, but a copy in `y`, a callee parameter, or a returned result can
still carry that origin. Continue those paths independently. The query still has
one selected binding: following its derived values does not turn it into a query
for all writes to every downstream variable.

For each path, record where propagation stops or continues beyond the current
analysis, with the relevant origin, carrier/input, condition, source span, and
witness. Use the following distinctions:

| Situation | Required behavior and ending record |
| --- | --- |
| An assignment replaces the tracked origin in one place | Kill that place's old fact. Continue other copies and control consequences. The assignment is not a global end for the origin. |
| No reachable read, transfer, or control consequence can use the remaining origin | Record `no_further_use` only with complete relevant analysis. An unread local value can end here without analyzing unrelated later operations. |
| The declared execution entry finishes, including modeled cleanup, without an escaping tracked value or pending consequence | Record `execution_exit` for that entry and path. This is not a claim that the entire application terminates. |
| A known callee returns a tracked value within the analyzed caller context | Follow its matched return site and later caller consumers. A callee's closing brace is not a stopping point. |
| A value escapes the declared entry via a return or another supported transfer, but its receiver is outside the analyzed context | Record `scope_boundary` with the escaping value and missing receiver context. Its later lifecycle is not established. |
| A named model ends at an external input/sink, such as a network request header | Record `modeled_boundary` with the exact input projection and model. The input flow is established; later external effects are not inferred. |
| A relevant call, mutation, exception, capture, or schedule is unsupported, or a budget is exhausted | Record `unknown_boundary` and the corresponding diagnostic. Preserve the known prefix and continue independent supported branches; do not claim no later impact. |
| A loop or recursion can keep carrying the origin | Reach a static fixed point and retain a compact cyclic witness. Record `cycle` where such continuation is supported; convergence is not proof that runtime execution ends. |

Apply these rules per path and per carrier. Closing one branch or overwriting one
copy does not close another. A claim that the entire value lifecycle has ended
requires all continuations to end by `no_further_use` or `execution_exit`; a cycle
or any boundary prevents that claim. Fully representing a cycle can still be a
complete static analysis of the query.

The roadmap controls which continuations can be followed: straight-line copies
in stage 1, branches/loops in stages 2–3, resolved calls and matched caller returns
in stage 4, and exceptions, aliases, captures, and deferred work in their later
stages. Encountering a later-stage construct does not allow a shorter flow to be
reported as a completed lifecycle. In stage 4, an explicitly included enclosing
caller context must also follow a value returned from the selected function into
that caller's later consumers; do not merge distinct caller contexts to extend it.

### Exact consumer inputs and external boundaries

Keep the exact part of an input that receives the selected value. For example,
these are two separate uses of `contentLangs`:

```text
contentLangs -> fetch URL's lang interpolation
contentLangs -> fetch options.headers["Accept-Language"]
```

The second use can be represented as
`arguments[1].headers["Accept-Language"]`, where argument positions start at zero.
Reporting only "the fetch options changed" would lose which field receives the
value. Another header can change independently of this language flow.

A known input to a call does not make its output known. We can show that
`contentLangs` reaches a request header without claiming which response the server
returns. Similarly, a modeled preference read can identify an input source without
claiming to know every earlier write to persistent storage. The report names the
model or unknown boundary used for either case.

## 6. Before/after comparison

Analyze both snapshots independently with identical capabilities and summaries.
Never propagate facts across revisions. Compare the same selected binding and
entry assumptions, using their cross-revision counterparts.

### 6.1. Identity and delta kinds

Align bindings and operations using an explicit selector, file/rename mapping,
lexical structure, syntax roles, and local edit context. Keep a matched operation's
logical identity separate from its semantic fingerprint: changing `let x = 1` to
`let x = 2` changes the initializer, not the identity of the declaration or write.
The fingerprint records the operation kind, resolved input and target bindings,
operators, literals, call target, and input/property projections. Source positions
and syntax trivia alone do not change that fingerprint. Moving an operation can
still change execution order and therefore flows, even if its fingerprint is unchanged.

Insertion of a repeated call must not silently shift every later call's identity.
If multiple correspondences remain credible, report `AmbiguousMatch` and
`AnalysisUnknown` for the affected candidates. Do not assert which old node was
removed or which new node corresponds to it. Unambiguous parts remain comparable.

| Delta | Meaning |
| --- | --- |
| `NodeAdded` / `NodeRemoved` | An operation is added to or deleted from the code, with sufficient identity evidence, and is relevant to the query. |
| `SliceMembershipChanged` | A matched operation exists in both snapshots, but is relevant to the selected binding in only one. Include `entered` or `left`; this is not code insertion/deletion. |
| `OperationChanged` | A matched operation relevant in either snapshot has a changed semantic fingerprint. |
| `WriteAdded` / `WriteRemoved` | An operation can write the selected binding/place in only one snapshot. This includes insertion/deletion, retargeting an existing assignment, or a proven change in its reachability. |
| `FlowAdded` / `FlowRemoved` | A typed relation from section 5 is established on one side and proven absent on the other, using the same logical endpoints and input/property projection. |
| `FlowConditionChanged` | The same typed relation exists on both sides, but the supported conditions under which it can occur differ. Include both conditions. |
| `ValueSourceChanged` | A matched consumer's input has a different set of upstream origins, or a value-producing operation on a supported dependency path to that input has a changed fingerprint. Include the input slot, changed origins/operations, and affected paths. |
| `AnalysisUnknown` | A specified node, write, relation, condition, or origin comparison cannot be decided. Include the affected side and diagnostic reason; this is not a positive change claim. |

### 6.2. Exact comparison rules

1. Compare each flow by `(relation kind, logical source, logical consumer,
   input/property projection)`. Conditions and example witness paths are attributes,
   not part of this identity. A relation present on both sides is retained, even
   when its conditions change. Do not replace it with a removed/added pair.
2. Compare presence separately from conditions and value sources. A retained
   `reaches` edge can have `ValueSourceChanged` at its consumer when its writer's
   RHS changes. If only guards change and the same origins and producing expressions
   remain possible, emit `FlowConditionChanged`, not `ValueSourceChanged`. If an
   origin becomes impossible, also report the removed dependency and changed source set.
3. Propagate value-source changes only along established value dependencies to
   the particular consumer input. A changed write that is overwritten before a
   read cannot change that read's sources. Control influence is reported through
   `controls`; it is not by itself a copied-value dependency.
4. Classify code existence, slice membership, and writing separately. A newly
   inserted assignment can have both `NodeAdded` and `WriteAdded`. A surviving
   assignment retargeted away from the selected binding has `OperationChanged`
   and `WriteRemoved`, not `NodeRemoved`. `SliceMembershipChanged` applies only
   to nodes whose existence on both sides has been established.
5. A write is an assignment event, not proof of a different runtime value.
   Inserting `x = x` still adds a write. Likewise, report changed source expressions
   without claiming their runtime results must differ; value-equivalence proofs
   are outside the initial analysis.

Compare path conditions using the supported guard rules, not predicate text alone.
Proven equivalent conditions produce no condition delta. If those rules cannot
establish equivalence or a difference, report the condition comparison as unknown;
do not infer `FlowConditionChanged` merely from an edited guard expression.

Presence has three states, assessed for the particular relation being compared:

- `present`: at least one supported or explicitly modeled path establishes it.
- `absent`: analysis of all relevant alternatives proves it cannot occur under
  the declared entry assumptions and models.
- `unknown`: unsupported behavior, unresolved identity, or an analysis limit
  prevents either conclusion.

| Before | After | Required presence result |
| --- | --- | --- |
| `absent` | `present` | `FlowAdded` |
| `present` | `absent` | `FlowRemoved` |
| `present` | `present` | Retained; compare conditions and value sources separately. |
| `absent` | `absent` | No flow delta. |
| `unknown` | Any state | `AnalysisUnknown` for presence comparison; no definitive addition/removal. |
| `present` or `absent` | `unknown` | `AnalysisUnknown` for presence comparison; no definitive addition/removal. |

A global `partial` result does not make every relation unknown: proven local
relations can still be compared. Conversely, an empty edge set in a partial region
is not an absence proof. Unknown guard alternatives can leave presence established
but the condition comparison unknown. A consumer's full source-set comparison is
unknown if either side has unresolved contributing origins; independently proven
individual flow deltas remain reportable.

Apply the same evidence rule to writes. Report `does_not_write` only after resolving
the target and all relevant effects of the operation under the entry assumptions.
An unresolved call is not a non-writer merely because no explicit assignment was
found. Source-level node insertion/deletion can still be known when its flow effects
are unknown; do not suppress the structural evidence or invent the missing flows.

### 6.3. Worked before/after cases

The snippets below are function bodies with the required parameters in scope.
`x` is the selected binding unless stated otherwise. Labels such as `A`, `B`,
and `R` designate corresponding source operations across snapshots; `R` names
the read in `return x`. Assume unique alignment, identical entry assumptions,
and complete support for each example's syntax. Branches, loops, and calls become
required only at their roadmap stages; earlier stages must report their boundaries.

The expectations below identify required and forbidden changes for the named
operations and relations, not every temporary introduced during lowering.

#### Case 1: Change an initializer, retain the writer-to-read edge

```typescript
// Before
let x = 1; // A
return x;  // R

// After
let x = 2; // A
return x;  // R
```

- Required: `OperationChanged(A)` for `1 -> 2`; `ValueSourceChanged(R)` naming
  the changed value-producing expression at A.
- Retained: `reaches(A, R)` and A's identity as the writer of `x`. The origin ID
  remains A; its expression fingerprint changes.
- Forbidden: `NodeAdded/Removed(A)`, `WriteAdded/Removed(A)`, or removal/addition
  of `reaches(A, R)`. Do not summarize this as the whole function's flow changing.

#### Case 2: Insert or remove an overwriting assignment

```typescript
// Before
let x = 1; // A
return x;  // R

// After
let x = 1; // A
x = 2;     // B: inserted
return x;  // R
```

- Required: `NodeAdded(B)`, `WriteAdded(B)`, `FlowRemoved(reaches(A, R))`,
  `FlowAdded(reaches(B, R))`, and `ValueSourceChanged(R)` from origin A to B.
- Retained: A still exists, writes `x`, and belongs to the query even though its
  assigned value is overwritten before R.
- Forbidden: `WriteRemoved(A)`, `NodeRemoved(A)`, or a remaining A-to-R value
  dependency in this example. B's literal does not depend on A.
- Reverse the edit: B has `NodeRemoved` and `WriteRemoved`; remove B-to-R,
  add A-to-R, and change R's source from B to A. Do not report A as newly added.

If the inserted line were `x = x`, B would still replace A as R's last writer,
but A would remain a transitive origin through B's RHS read. Removing a direct
`reaches` edge must not automatically remove every transitive dependency.

#### Case 3: A surviving assignment stops writing the selected variable

```typescript
// Before
let x = 1; // A
let y = 0;
x = 2;     // B
return x;  // R

// After
let x = 1; // A
let y = 0;
y = 2;     // B: same assignment position, changed target
return x;  // R
```

- Required: `OperationChanged(B)` with target `x -> y`, `WriteRemoved(B, x)`,
  B's `SliceMembershipChanged(left)`, removal of `reaches(B, R)`, addition of
  `reaches(A, R)`, and `ValueSourceChanged(R)` from B to A.
- Forbidden: `NodeRemoved(B)` or `WriteAdded(B, x)`. B survives and now writes
  `y`; this one-binding query must not describe that as an added write to `x`.
- Reversing this edit makes B enter the slice and adds its write to `x`, without
  adding B to the code.

#### Case 4: Switch between existing input sources

```typescript
// Both snapshots have parameters p: string and q: string.
// Before
let x = p; // A
return x;  // R

// After
let x = q; // A
return x;  // R
```

- Required: `OperationChanged(A)`; removal of the value dependencies from input
  p into A and R; addition of those from input q; `ValueSourceChanged(R)` with
  origins `{p, A} -> {q, A}`. Here p/q abbreviate their parameter-entry sources.
- Slice membership: p's input source leaves and q's enters. Both parameters
  exist in both snapshots; neither has `NodeAdded` or `NodeRemoved`.
- Retained: `reaches(A, R)` and the write to `x` at A. The changed upstream input
  does not create a new declaration of `x`.

#### Case 5: Change only which branch supplies each value

```typescript
// Before
let x = 0; // A
if (flag) { // G
  x = 1; // B
}
return x; // R

// After
let x = 0; // A
if (!flag) { // G
  x = 1; // B
}
return x; // R
```

`flag` is an unknown boolean input and both outcomes are admissible.

| Relation | Before condition | After condition | Required delta |
| --- | --- | --- | --- |
| `reaches(A, R)` | `!flag` | `flag` | `FlowConditionChanged` |
| `reaches(B, R)` | `flag` | `!flag` | `FlowConditionChanged` |
| B may write `x` | `flag` | `!flag` | `FlowConditionChanged` |
| G controls B | `flag` | `!flag` | `FlowConditionChanged` |

Also require `OperationChanged(G)`. Both writers and both reaching relations
remain possible; R's possible origins remain `{A, B}` with unchanged producing
expressions. Do not emit `WriteAdded/Removed`, `FlowAdded/Removed` for these
retained relations, or `ValueSourceChanged(R)` solely for the guard change.

#### Case 6: Add a write inside a loop without losing the zero-iteration flow

```typescript
// Before
let x = 0; // A
while (again) {
  again = false;
}
return x; // R

// After
let x = 0; // A
while (again) {
  x = 1; // B: inserted
  again = false;
}
return x; // R
```

`again` is a boolean parameter. Let `again_entry` mean its value on entry; this
loop executes zero or one time because its body sets `again` to false.

- Required: `NodeAdded(B)`, `WriteAdded(B)`, and `FlowAdded(reaches(B, R))`
  under `again_entry`; `ValueSourceChanged(R)` from `{A}` to `{A, B}`, with B's
  contribution restricted to that path.
- Retained with changed condition: `reaches(A, R)` holds unconditionally before,
  but only under `!again_entry` after. Emit `FlowConditionChanged`, not `FlowRemoved`.
- Add the `controls` relation from the existing loop condition to B. Preserve
  the loop's identity; do not report the loop itself as inserted.
- Do not invent writers for iterations or the loop exit. Static write B has one
  identity, including in later fixtures where a loop can execute repeatedly.

#### Case 7: Change an upstream helper and preserve exact consumer inputs

The [PR #5816 case study](PR-5816.md) selects `contentLangs`. Its initializer changes
from `getContentLanguages().join(',')` to `getAppLanguageAsContentLanguage()`.
The first request still reads `contentLangs` in its URL's `lang` interpolation
and in `arguments[1].headers["Accept-Language"]`.

- Require `OperationChanged` at the matched initializer and `ValueSourceChanged`
  at each of those two surviving inputs. Retain their immediate reads from the
  `contentLangs` write; a different helper does not remove those reads.
- Remove the old upstream dependencies through the content-language preference
  and `join`; add those through the app-language preference and the new helper's
  `split('-')[0]`, when the named call and library models are available.
- `getContentLanguages` still exists in the repository. Its previously relevant
  operations leave this query's slice; do not report the helper as deleted.
- `getAppLanguageAsContentLanguage` is actually introduced by this PR. Its relevant
  new operations have `NodeAdded`, not merely `SliceMembershipChanged(entered)`.
- The independent added labeler header is not another `contentLangs` flow. The
  fallback request does not gain a language input. Unknown server behavior must
  not be converted into a proven input-to-response dependency.

#### Case 8: Edits with no downstream value-source change

| Before | After | Required result for selected `x` |
| --- | --- | --- |
| `let x = 1; x = 3; return x;` | `let x = 2; x = 3; return x;` | `OperationChanged` at the initializer, but no `ValueSourceChanged` at the return. Only the unchanged `x = 3` supplies that read. The initializer still writes `x`. |
| `let x = 1; let y = 2; return x;` | `let x = 1; let y = 3; return x;` | Empty query delta: the edited write to `y` has no dependency or control relationship with `x`. |
| `let x=1; return x;` | `let x = 1; /* note */ return x;` | Empty query delta. Report updated locations if necessary, not semantic changes. |

#### Case 9: Incomplete analysis cannot prove that a flow disappeared

Suppose a known upstream source S reaches a surviving consumer R before the edit.
After the edit, resolving the source-to-R path crosses an unsupported call.

- Preserve the established before relation in `before_graph`.
- Mark that after relation `unknown` and emit `AnalysisUnknown` with the call
  location and reason, such as `UnresolvedCall`. Do not emit `FlowRemoved(S, R)`.
- If the edited call is uniquely matched, its `OperationChanged` is still known.
  Compare other fully established relations normally.
- In the reverse situation, an unresolved before path and established after
  path do not prove `FlowAdded`. If R has unresolved contributing origins, do
  not present its full source set as exhaustively compared.

#### Case 10: Follow an origin change through an unchanged consumer chain

```typescript
// Before
let x = 1;              // A: selected binding
const copy = x;         // B
x = 0;                  // C
const result = copy + 1; // D
return result;          // R

// After
let x = 2;              // A: only edited line
const copy = x;         // B
x = 0;                  // C
const result = copy + 1; // D
return result;          // R
```

- Required: `OperationChanged(A)` and `ValueSourceChanged` for the RHS input
  at B, the `copy` operand at D, and the `result` read at R. Keep the complete
  A-to-B-to-D-to-R value-dependency chain in both graphs, even though B, D, and
  R were not edited and none reads `x` after C.
- Retained: the immediate writer-to-read edges along that chain. Changed
  upstream provenance does not make B, D, or R newly inserted operations.
- Forbidden: a C-to-D or C-to-R value dependency. Reassigning `x` at C does not
  change the earlier copy. Do not end A's downstream analysis at B or C.
- With the default containing-function entry, R exports the derived value to
  an unspecified caller. Record `scope_boundary`, not `no_further_use`. With
  an included, resolved caller context at stage 4, continue through the matched
  return site to that caller's consumers.
- If D was involved in an earlier bugfix, the retained location, expression,
  and dependency chain provide evidence for later historical correlation.
  This report does not by itself establish that D is buggy now.

### 6.4. Grouping and required summary content

Assign a deterministic group ID to each matched edited operation or confirmed
inserted/deleted operation that explains a change. Derive it from the query,
snapshot pair, and logical operation identity, not discovery order. Attach related
node, write, condition, and consumer deltas to that group. When one derived delta
has several contributing edits, emit the delta once and reference all applicable
group IDs. Do not guess a causing edit when evidence is incomplete.

A human summary must name the changed operation, whether its write to the selected
binding was added/removed/retained, affected consumer input(s), before/after sources
or conditions, and any unknown boundary. Omit fields that are inapplicable rather
than inventing them. Do not replace these facts with "the function changed" or
"data flow changed". For case 2, the required level of precision is:

> Added assignment B (`x = 2`). It replaces A as the last writer reaching the
> `return x` read R. A remains in the code and still writes `x`; its value no longer
> reaches R.

Structural and derived records describe different aspects of this one edit;
grouping must not turn them or multiple witnesses into repeated findings. Canonical
flow-claim scoring remains as defined in the [benchmark specification](BENCHMARKS.md).

Keep the full upstream/downstream graphs separate from the delta list. A node or
edge retained as context is not an extra change claim. For later consumer-history
correlation, preserve each consumer's repository path, enclosing declaration,
input projection, expression fingerprint, snapshot-local identity/span, and its
aligned before/after identity where established. Retain the path from the edited
origin to that consumer and the continuation after it, including ending records.
Pair-local logical IDs must not be assumed to identify the same code across
arbitrary historical revisions; any later history comparison needs its own
alignment evidence and compatible scope/model information.

## 7. Results, evidence, and uncertainty

| Report field | Required content |
| --- | --- |
| `schema_version`, `analysis_version` | Versioned output and implementation/model identifiers. |
| `snapshots`, `query`, `entry_assumptions` | Reproducible inputs and resolved selected bindings. |
| `before_graph`, `after_graph`, `alignment` | Full supported upstream/downstream slice, including unchanged intermediate and final consumers, typed edges, locations, fingerprints, and cross-revision mapping evidence. |
| `deltas` | Typed changes with before/after endpoints, logical IDs, conditions, source spans, and group IDs. |
| `witnesses` | Compact valid paths or summary expansions supporting a relation; cycles represented by backedges. |
| `diagnostics`, `unknown_frontiers` | Reasons, affected files/nodes/flows, and the next unsupported operation. |
| `capabilities`, `summaries_used` | Exact supported behavior and audited boundary/library assumptions. |
| `flow_extent` | Upstream/downstream coverage separately for each snapshot, declared entry/caller scope and source/sink boundaries, and whether value-lifecycle closure is established. |
| `path_endings` | Per-origin/carrier endings or open continuations from section 5: kind, location, condition/context, witness, and model or diagnostic where applicable. |
| `completeness`, `stats` | Overall and per-region completeness, counts, timing, limits hit, graph/witness truncation. |

Use categorical evidence, not invented per-flow probability scores:

- `supported`: complete derivation in supported semantics, including resolved
  call targets, admissible entry assumptions, and satisfied required guard checks.
- `modeled`: relies on a named, versioned library/source/sink summary; disclose it.
- `unresolved`: feasible only under unknown targets, effects, guards or alignment;
  belongs in uncertainty output and is not worded as an established dependency.

Global completeness is `complete_for_query`, `partial`, or `unsupported`. A complete
query still has explicit input/sink boundary assumptions and is not a whole-program
correctness proof. A complete relation may coexist with a partial downstream region.
`complete_for_query` within declared source/sink or entry-scope boundaries does
not mean the complete value lifecycle is known. `flow_extent` must expose that
distinction: an established request-input slice can be complete under its models
while the continuation beyond the request remains open. Unsupported continuations
inside the declared scope make the affected direction/region partial. A summary
must not say "no further impact" when it merely reached a boundary.

Diagnostic codes include `UnsupportedSyntax`, `UnresolvedBinding`, `UnresolvedCall`,
`UnknownExternalEffect`, `UnknownHeapEffect`, `UnknownSchedule`,
`UnprovenPathFeasibility`, `AmbiguousMatch`, `MissingDependency`, `ParseError`,
`TypeResolutionUnavailable`, `AccessPathLimit`, `AnalysisBudgetExceeded`, and
`OutputTruncated`. Include a human explanation and affected frontier for each.
An unsupported operation cannot be replaced by a no-op because it was not edited.

The solver may retain conservative candidate relations internally. Validate paths
against supported guard rules before presenting supported flow claims. Proving one
witness infeasible does not eliminate other possible witnesses. If all alternatives
cannot be decided within the supported theory/budget, report unresolved feasibility.
Prune only justified impossibilities. This refinement layer must not covertly add
non-distributive rules to IFDS transfers.

## 8. Resource behavior and acceptance

Index metadata once per immutable snapshot. Cache summaries by body, dependency,
configuration and model hashes; changes invalidate callers and dependent summaries.
Slice/query optimizations must preserve the required definitions and origins;
cross-check against an unsliced reference analysis on small supported programs.

Starting engineering targets, to calibrate in stage 0 and document when revised:
one query over a pinned medium-sized application, cold p95 at most 120 seconds,
warm p95 at most 15 seconds, peak process-tree memory at most 4 GiB on an 8-core,
16-GiB machine. Measure file/LOC counts and include metadata-provider costs. These
are design targets, not observed performance or a claim about current Bluesky size.

Default limits should be explicit (initial proposal: 120 seconds, 4 GiB, one million
processed path edges, three example witnesses per relation). Reaching an analysis
limit yields a partial report; it cannot certify absent flows. Limiting displayed
witnesses may leave analysis complete, but must declare omitted witnesses. Truncating
flow/graph output must be distinguishable from a complete report of no changes.

Acceptance requires at least 90% precision on reviewed reported-flow changes,
exact promoted semantic fixtures, separately visible recall/coverage, and correct
uncertainty behavior. [Benchmark specification](BENCHMARKS.md) defines scoring.
The number is an empirical target on a declared suite, not an IFDS theorem or a
per-finding likelihood. [Roadmap](README.md) defines the staged implementation.
