# Function result acceptance test examples

Status: fixture sketches for [FR001–FR003](README.md). The required unit-test
families and function-result integration suite are implemented for the first subset.

Each case below gives a compact fixture idea and its expected semantic assertion.
These are reviewable oracles, not executable Rust tests. Convert them to in-memory
before/after repository fixtures when implementing. Names such as
assert_equal_region describe evidence to check, not proposed API functions or
serialized field names.

Unless a case says otherwise, the selected function is uniquely matched,
same-position parameters are paired inputs, capabilities and summaries match across
snapshots, and branch feasibility is supported. Simple guards over paired input
reads use symbolic JavaScript truthiness without an entry-domain restriction;
examples that specifically test declared domains say so. A result means the
whole normally returned value.

## FR001 — Function query and result dependencies

### function_selector

~~~typescript
function first(flag: boolean) { if (flag) return "a"; return "b"; }
function second(flag: boolean) { if (flag) return "c"; return "d"; }
~~~

Select the exact declaration span of first in both snapshots. Assert that only
first and its two return observations enter the query. A stale selector, reference
span, or mismatched name/owner assertion fails. For a separate function named
café, a byte span ending inside the UTF-8 encoding of é is invalid. Two equally
plausible counterparts produce unresolved matching; do not choose by name or
source order.

### input_correspondence

~~~typescript
// Before
function choose(preferred: string) { return preferred; }
// After
function choose(language: string) { return language; }
~~~

With one simple parameter, assert equal results for the same positional string
input without requiring an explicit mapping. The rename does not change which
runtime argument supplies the return. Record and round-trip its entry-domain
assumption. A reorder of two simple parameters is also compared by argument
position, as this concrete-call variant shows:

~~~typescript
// Before, when called with (1, 2)
function choose(first: number, second: number) { return first; }
// After, for the same call arguments (1, 2)
function choose(second: number, first: number) { return first; }
~~~

Assert before returns 1 and after returns 2. The unchanged name first now
receives the second argument. In a separate changed-arity variant, add a leading
parameter and return an additional, otherwise unmapped argument; leave that
comparison unresolved unless supported call-site evidence establishes its value.

### entry_assumption_expressions

~~~typescript
// Both snapshots
function answer(flag: boolean, ready: boolean) { return 1; }
~~~

Declare the before entry domain as the typed expression `arg0 && arg1` and the
after entry domain as `arg0`, where `arg0` and `arg1` are positional Boolean
inputs. Assert that both expression trees, their input references and operand
roles, and their different snapshot versions survive serialization. FR001 keeps
both assumptions and their dependencies; FR002 decides the common domain. In a
separate numeric-input variant, also record an arithmetic predicate such as
`arg0 + 1 > 0` as a structured expression. If the enabled theory cannot decide its
feasibility, preserve that predicate and mark the affected coverage unresolved.

### literal_without_binding

~~~typescript
function answer() { return 1; }
~~~

Assert one unconditional result with value 1, no binding selector, and no
invented input origin.

### all_normal_exits

~~~typescript
function exits(a: boolean, b: boolean, c: boolean) {
  if (a) return 1;
  if (b) return;
  if (c) return 2;
}
const arrow = (value: number) => value + 1;
~~~

For exits, assert 1 under a, bare-return undefined under !a && b, 2 under
!a && !b && c, and fallthrough undefined under !a && !b && !c. Selecting arrow
separately yields the whole expression value + 1. Keep the two undefined exits
as distinct observations.

### nested_and_unreachable

~~~typescript
function outer(flag: boolean) {
  function nested() { return 3; }
  if (flag) return 1;
  return 0;
  return 2;
}
~~~

Assert outer results {1 under flag, 0 under !flag}. The later return 2 is
unreachable; return 3 belongs to nested. Neither is an outer result. In the
after snapshot, change only nested's return from 3 to 4. Assert the outer
result regions remain equal and that the nested edit does not enter its
dependency slice.

### whole_expression

~~~typescript
function calculate(input: number) { return input + 4; }
~~~

Assert that input + 4 is the whole result with operand roles input and literal 4.
Do not report input alone as the returned value or omit the literal.

### overwrites_and_copies

~~~typescript
function overwritten(input: number) {
  let value = input; // A
  value = 3;         // B
  return value;
}
function copied() {
  let value = 1; // A
  const saved = value; // B
  value = 3;         // C
  return saved;
}
~~~

Assert overwritten returns B's literal 3; A cannot reach it. Assert copied
returns the value captured by B from A; C does not replace the copy. Changing A
from 1 to 2 in copied changes its result even though value is overwritten.

### effective_guards

~~~typescript
function choose(flag: boolean) {
  let value = "old"; // A
  if (flag) value = "new"; // B
  return value;
}
function early(flag: boolean) {
  if (flag) return "early";
  return "late";
}
function short(flag: boolean) { return flag && "yes"; }
function nested(a: boolean, b: boolean, c: boolean) {
  let value = 0;
  if (a) { if (b) value = 1; }
  if (c) value = 2;
  return value;
}
~~~

Assert choose returns A under !flag and B under flag. The guard controls B but
is not an origin of the literal "new". Assert early's outcomes are flag and
!flag. Assert short returns false under !flag and "yes" under flag; its right
operand is skipped on the false path. In a reassigned-guard variant, use the
value version at the condition. Assert nested returns 2 under c, 1 under
!c && a && b, and 0 under !c && (!a || !b). The later c write overrides the
nested write when both execute.

### unknown_completion

~~~typescript
function maybeThrows(input: string) {
  mystery(input);
  return input;
}
~~~

Treat mystery as unresolved. Keep the known argument dependency and return on
the continuing path, but expose an unknown completion boundary. Do not assert
purity, guaranteed return, or equivalence to return input. Throw and possibly
nonterminating variants remain unknown until modeled.

### scope_and_limits

~~~typescript
function identity(input: string) { return input; }
function caller() {
  const value = identity("en");
  return value;
}
~~~

Without caller context, the identity return observation is complete while its
value has an open caller continuation. With an injected analysis limit, affected
dependency coverage is partial and the limit reason is visible.

## FR002 — Conditions and result comparison

### guard_added

~~~typescript
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
~~~

With no declared Boolean entry domains, assert exactly: truthy enabled and ready
gives "ok" -> "ok", equal; truthy enabled and falsy ready gives "ok" -> "skip",
different; falsy enabled gives "skip" -> "skip", equal. The possible-value set
is unchanged, but that must not hide the changed middle region.

### equal_branch_values

~~~typescript
// Before
function same(flag: boolean) { if (flag) return 1; return 1; }
// After
function same(flag: boolean) { if (!flag) return 1; return 1; }
~~~

Assert unconditional equal result and changed condition/control. Even if
individual return sites are ambiguous, the complete summaries prove equality.

### early_return_and_undefined

~~~typescript
// Before
function status(flag: boolean) { return 2; }
// After
function status(flag: boolean) { if (flag) return 1; return 2; }
~~~

Assert different (2 -> 1) under flag and equal (2 -> 2) under !flag. Also
compare if (flag) return 1; with if (flag) return 1; return 2; and assert
undefined -> 2 under !flag while the flag region stays equal.

### return_shape

~~~typescript
// Before
function value(flag: boolean) { return flag ? 1 : 2; }
// After
function value(flag: boolean) { if (flag) return 1; return 2; }
~~~

Assert equality under both flag and !flag despite different return structure.
Uncertain site alignment must not erase independently proven result equality.

### same_input_and_copy

~~~typescript
// Before
function echo(input: string) { return input; }
// After
function echo(input: string) { const copy = input; return copy; }
~~~

Assert equality for the paired input and retain the after-side copy dependency.
For the distinct-input variant below, both parameters are paired by position,
but p and q remain independent unknown values. Assert changed return-source
selection, without claiming the concrete values are unequal:

~~~typescript
// Before
function choose(p: string, q: string) { return p; }
// After
function choose(p: string, q: string) { return q; }
~~~

### expression_changed

~~~typescript
// Before
function add(input: number) { return input + 1; }
// After
function add(input: number) { return input + 2; }
~~~

Assert changed return computation over the supported normal-return region.
Retain both expressions, their paired input dependency, and the edited literal
operand. Do not evaluate the addition or assert different numeric values for
every input; floating point rounding can erase a small increment at large
magnitudes. This case is changed, not unknown. As a control, compare the same
pure input + 1 expression in both snapshots under a paired primitive-number
input domain and assert equal without computing a concrete sum.

### guard_versions_and_order

~~~typescript
// Before
function selected(flag: boolean) {
  let guard = flag;
  if (guard) return "yes";
  return "no";
}
// After
function selected(flag: boolean) {
  let guard = flag;
  guard = !guard;
  if (guard) return "yes";
  return "no";
}
~~~

Assert "yes" -> "no" under flag and "no" -> "yes" under !flag, using the
reassigned guard value. Also reverse two writes:

~~~typescript
// Before
function priority(a: boolean, b: boolean) {
  let result = 0;
  if (a) result = 1;
  if (b) result = 2;
  return result;
}
// After
function priority(a: boolean, b: boolean) {
  let result = 0;
  if (b) result = 2;
  if (a) result = 1;
  return result;
}
~~~

Assert equal when neither or just one flag is true, and different (2 -> 1) only
when both are true. Preserve both sources and the changed overwrite precedence.

### guard_changes_return

~~~typescript
// Before
function value(flag: boolean) { if (flag) return "yes"; return "no"; }
// After
function value(flag: boolean) { if (!flag) return "yes"; return "no"; }
~~~

Assert the condition edit flag -> !flag and a finding that links it to the changed
return selection without a declared Boolean entry domain. For the same paired
input, truthy flag changes the result from "yes" to "no"; falsy flag changes it
from "no" to "yes". The distinct literals support different results on both
regions. No concrete flag evaluation or general predicate solver is required:
symbolic truthiness is sufficient. In particular, the two "yes" return paths cannot be
treated as one common input region, since their guards are flag and !flag.
Keep the result consequence alongside the condition change in text and JSON.

### common_domain

~~~typescript
// Both versions
function enabled(flag: boolean) { if (flag) return 1; return 0; }
~~~

Declare the before domain {false, true}, after domain {true}. Assert equality
only on the common domain flag = true, disclose false as excluded, and forbid
a whole-before-domain equality claim.

### entry_assumption_change

Reuse `answer` from entry_assumption_expressions. The before assumption is
`flag && ready`; the after assumption is `flag`. Assert an equal result only on
their proven common domain `flag && ready`. Report `flag && !ready` as an
after-only query domain and `!flag` as outside both query domains. Show the
before/after assumption expressions as a scope change, without attributing them
to a source-code edit or comparing an absent before-side result.

### empty_common_domain

~~~typescript
// Before
function answer(flag: boolean) { if (flag) return "old"; return "off"; }
// After
function answer(flag: boolean) { if (flag) return "new"; return "off"; }
~~~

Declare `flag = true` before and `flag = false` after. Assert `no common inputs`
and no `equal`, `different`, or `changed` result relation. Display the before
flow `flag -> "old"` under its own domain and the after flow
`!flag -> "off"` under its own domain, with their return observations and
dependencies. Do not present `"old" -> "off"` as a same-input value change.

### independent_unknown_region

~~~typescript
// Before
function mixed(change: boolean, unsupported: boolean) {
  if (change) return 0;
  mystery(unsupported);
  return 1;
}
// After
function mixed(change: boolean, unsupported: boolean) {
  if (change) return 2;
  mystery(unsupported);
  return 1;
}
~~~

Assert supported different (0 -> 2) under change. Retain unknown completion for
the other region because of the call. Do not claim full result coverage.

### presence_not_value

~~~typescript
// Before: the complete indexed snapshot has no matching function.
// After
function newResult() { return undefined; }
~~~

With addition confirmed, report function presence and its after-side result; do
not compare against fabricated before-side undefined. Ambiguous counterparts
remain unresolved.

### primitive_semantics

Use independent relation-layer value fixtures. Assert equal for string "x" versus
string "x" and different for number 1 versus Boolean true. Until special numeric
values have an explicit model, NaN versus NaN and positive versus negative zero
stay unknown. Once modeled, the chosen SameValue result rule makes the former
equal and the latter different. Unknown objects with identical printed text stay
unknown; do not infer coercion or object identity from source spelling.

### declared_input_scope

~~~typescript
// Before
function answer(flag: boolean) { if (flag) return 1; return 0; }
// After
function answer(flag: boolean) { if (flag) return 2; return 0; }
~~~

Declare the query input domain as flag = true. Assert the changed result 1 -> 2
for that input and record that flag = false is outside the query scope. Do not
claim a comparison for flag = false or generalize the finding to all Boolean
inputs. This query is complete within its declared domain.

## FR003 — Reports and end-to-end acceptance

Reuse this fixture; its three expected regions are specified under guard_added:

~~~typescript
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
~~~

These tests assert semantic facts, not proposed JSON field names.

Use this additional fixture to check compact presentation of affected return
choices with multiple enclosing conditions:

~~~typescript
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
~~~

### api_round_trip

Run the public analysis, serialize the full report, then reload it. Assert
semantic equality for target, snapshots, paired inputs, all result regions,
assessments, dependencies, conditions, evidence, coverage, and limits. Include
the entry_assumption_change fixture to cover distinct scope expressions and the
expression_changed fixture to cover the changed assessment. Repeat with a
partial fixture and ensure unknown reasons and open-scope records survive.

### text_json_parity

Render text and compact JSON. Both identify the added ready check, the
"ok" -> "skip" change under enabled && !ready, and equality in the other two
regions. For expression_changed, both say the return computation changed from
input + 1 to input + 2 without claiming a numerical difference for all inputs.
For guard_changes_return, both show flag -> !flag and the corresponding swap
between "yes" and "no" for the same symbolic input.
For entry_assumption_change, both show the common `flag && ready` domain and the
after-only `flag && !ready` domain as a query-scope change, without inventing a
source-code edit.
For status, both show when "ok" and "pending" are selected before and after,
factoring enabled && !blocked into one explicit common context. Both retain the
context association for every relative selection and effect condition, so the
"ok" -> "pending" region reconstructs to
enabled && !blocked && ready && !approved.
They summarize "disabled" and "blocked" as unchanged without expanding their
equal regions in the compact presentation.
On an unsupported-call fixture, both expose the boundary; neither may
summarize a partial comparison as unchanged.

### deterministic_evidence

Use a fake clock and fixed resource statistics, then permute independent
processing order. Assert byte-identical canonical output and stable evidence IDs.
With real elapsed-time telemetry, compare stable semantic fields rather than
requiring byte equality. Resolve every compact reference into that run's full
report and confirm it points to the claimed condition, result, or diagnostic.

### compact_grouping

Assert one semantic finding for the enabled && !ready result change, supported
by changed-condition and before/after-return evidence. Keep both equal regions as
context. For status, display enabled && !blocked once as the common context.
Show this table with conditions explicitly relative to that context:

| Choice | Before | After |
| --- | --- | --- |
| "ok" | ready | ready && approved |
| "pending" | !ready | !ready \|\| !approved |

Report the effect within that context: ready && !approved changes "ok" to
"pending". Assert that combining the context with each relative condition
recovers the complete before/after selection rules and changed-result region.
Do not repeat the common context in every row or add a condition-edit sentence
that duplicates the table. Summarize the proven unchanged "disabled" and
"blocked" choices without listing their input regions in the compact view.
Keep source locations and derivations accessible through detail references.
Unknown results and incomplete coverage remain visible in the compact view.
Do not repeat a finding for every edge or witness, and do not call unexamined
choices unchanged when coverage is partial.

### attribution_and_scope

~~~typescript
// Before
function value(flag: boolean) {
  if (flag) return 1;
  return 2;
}
// After
function value(flag: boolean) {
  if (flag) return 1;
  return 3;
}
~~~

Assert the !flag result change links to the retained final return with its edited
literal and correct before/after spans. In a separate split/merge fixture with
indistinguishable returns, report uncertain site attribution rather than guessing
which source edit corresponds. Without caller context, keep the continuation open
even if the function comparison is complete.

### coverage_and_truncation

Give a function several result regions and a small presentation budget. Assert
omitted groups are counted and retrievable. Separately use a function with four
independent Boolean inputs and distinct normal results for its 16 input
combinations. In separate runs, set the analysis budget and the comparison
budget so each run stops after at least one region is established but before all
16 are checked. Preserve those established facts, mark the remaining regions
and overall coverage partial, and expose which budget was exhausted even when
presentation has room. Do not assume which region is processed first. An empty
partial delta list must not render as unchanged behavior.

### binding_compatibility

Run an existing selected-binding fixture. Assert its query fields, full graph,
VariableFlowReport and VariableSourceReport schemas, compact source facts, output
choices, and command invocation retain their existing meaning. Function
selection uses `--function <name>` with optional `--after-function <name>` for an
explicitly renamed counterpart; never infer it by changing the old binding
selector's interpretation. A duplicate function name in either selected snapshot
fails clearly in the command; exact source-span selection remains available in
the public API.

## Promoting sketches to executable tests

Write independent fixture expectations for every named case before accepting
implementation. Reuse snippets only when doing so cannot hide an unrelated
failure. Include the negative and uncertainty variants stated above. Choose Rust
helper names, report fields and serialized schemas from the implemented API;
these illustrative assertions are not an existing interface.
