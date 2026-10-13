# K018: matched direct calls

`analyze_variable_flow_with_metadata` and `analyze_function_result_with_metadata`
now follow resolved synchronous direct calls into supported snapshot bodies. This
includes other files, nested calls and immutable direct function initializers.
The APIs without metadata retain local unknown-call behavior.

The K017 provider establishes the invocation target, not its effects. Missing,
excluded, external, declaration-only and unresolved targets remain boundaries.
Captured data and unsupported parameter forms cannot become supported calls just
because the compiler identifies their declarations. Unknown effects inside a
known body retain the supported dependency prefix and an explicit boundary;
transitive effect classification never infers purity from a signature.

## Transfers and solving

The language-independent solver records `(procedure, entry fact, node, fact)`
relations. Calls register deduplicated waiters for compatible entry facts and
reuse entry/exit summaries. New exits resume matching waiters. These records
contain static identities, not activation IDs or runtime stacks. Summaries are
per-analysis dependency relations; persistent caching belongs to K038.

Zero enters the callee and seeds formal reaching-write facts. Actual origins map
to the corresponding parameters; callees do not acquire invented default
function-input origins. Missing scalar arguments, bare returns and fallthrough
produce `undefined`. Additional arguments are evaluated normally. Return origins
map to a separate result temporary at that call's return site. Assigning the
result remains an ordinary caller write, so assigning to a formal parameter
cannot rewrite the caller binding.

Only proven unaffected caller facts survive bypass. Normal continuation waits
for a matching normal callee exit. Unsupported callee effects conservatively
invalidate the mutable caller places identified by lowering while preserving
independent supported facts.

## Evidence and branch validation

After tabulation, the report adapter expands nonrecursive bodies into distinct
static namespaces per call site. This is an evidence projection for the existing
branch validator, not an activation stack in the IFDS fact domain. The projection
uses origin-free `CallBind` transfers for actual/formal and returned-value
carriers, so these boundaries do not fabricate assignment origins. Ordinary
callee and caller assignments keep their existing origin-generation rules.

The branch validator checks the expanded actual/formal context, including known
arguments and multiple guarded returns. A callee return continues into its own
caller's consumers; it does not become a result observation or scope ending for
the enclosing query. File and call namespaces prevent collisions between
otherwise identical local IDs and source spans. Repeated callee operations are
aligned through matched call-result producers; unpaired contexts remain uncertain.
Source snippets are read from their own snapshot files.

Variable-flow witnesses carry `call_contexts`, ordered outer-to-inner call IDs,
and name their matched callee/return-site summary expansions. Contexts also attach
to caller continuation witnesses through their data dependencies. Function-result
`ResultDependency` records carry an ordered `call_context`, including dependency
records below caller result temporaries. Branch-independent bypass through an
unrelated call does not make that call a value dependency.

The variable evidence schema is now **5**, function analysis schema **6**, and
function full-report schema **7**. Compact variable/function schemas remain 3/6;
the metadata envelope remains schema 1. Readers reject unsupported full-report
versions as before. Local reports include empty call-context collections.

Tabulation and projection share query budgets. Evidence expansion is bounded by
output-node and memory limits; exhausted work retains a diagnosed open boundary.
Recursive evidence expansion remains an explicit boundary pending K019. Selecting
an entry call or adding an enclosing caller for a selected callee remains K020.
No module initialization, heap/capture effects or external library purity model is
introduced by this task.

## Verification

The required K018 tests use in-memory IR and authored metadata targets. Additional
cases cover nested files, missing arguments, bare/fallthrough results, guarded
returns, captured effects, transitive unknown effects, budgets and both metadata
APIs. The integration gate uses the actual pinned TypeScript bridge on immutable
tiny projects; it executes no application or package scripts.

```sh
cargo test --locked --offline --lib ifds_k018_ -- --list
cargo test --locked --offline --lib ifds_k018_
cargo test --locked --offline --test ifds_calls
cargo fmt --check
cargo test --locked --offline
cargo clippy --locked --offline --all-targets -- -D warnings
```
