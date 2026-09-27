# FR001 — Function query and result dependencies

Status: implemented for the first synchronous single-function subset.

Dependencies: [K013](../kickoff/013-analysis-api.md),
[K014](../kickoff/014-branches.md), [K015](../kickoff/015-short-circuit-expressions.md).

Source: [function result specification sections 1–3](../../docs/ifds/FUNCTION_RESULTS.md).

Fixture examples: [FR001 acceptance test sketches](TEST_EXAMPLES.md).

## Scope

- Add a public function-result query with function selectors and explicit
  counterparts, plus internal per-snapshot result observations and dependencies.
  Preserve existing binding query/API schemas; full result reports belong to FR003.
- Represent entry assumptions as typed expressions over positional input slots,
  including their operand dependencies and before/after versions. Preserve
  supported Boolean/primitive expressions and known values through serialization;
  unsupported predicates remain explicit uncertainty, never silently discarded.
- Infer counterparts only when current path/name/owner evidence identifies one
  function. Require an explicit counterpart for a renamed function; automatic
  rename tracking is a later feature.
- Seed all normal return observations owned by the selected procedure, including
  literals, bare returns, expression-bodied arrows, and reachable fallthrough.
- Reuse forward flow facts to build complete result dependencies and effective
  guards, preserving expression operands, overwrite rules and value versions.
- Record unresolved completion/dependencies and distinguish result completeness
  from the returned value's open caller continuation.

Excludes result-equivalence proofs, loops, resolved interprocedural expansion,
exceptions, and async/generator result semantics. Class and object methods are
owned by K031; this task selects ordinary function declarations, function
expressions, and arrows.

## Required unit tests

| Test suffix | Required evidence |
| --- | --- |
| `function_selector` | Exact declaration/expression spans select one ordinary function or arrow; stale hashes, invalid UTF-8 spans, and name/owner assertion mismatches fail. Ambiguous counterparts stay unresolved; function renames require an explicit counterpart. Methods are outside this selector subset. |
| `input_correspondence` | Match simple formals through positional actual arguments: a parameter rename retains the input, while a reorder can move a named formal to a different input. Changed arity/forms require supported argument semantics or justified assumptions. Entry-domain assumptions survive serialization. |
| `entry_assumption_expressions` | Before/after assumptions retain typed expression trees over positional inputs, operand roles, declared value domains, and distinct versions through serialization. Supported conditions constrain reachability; unsupported conditions mark affected coverage unknown. |
| `literal_without_binding` | `return 1` is a complete value observation without a selected variable. |
| `all_normal_exits` | Multiple early returns, bare return, arrow result, and reachable fallthrough have exact conditions and distinct observation identities; undefined differs from missing data. |
| `nested_and_unreachable` | Nested function returns and unreachable returns do not become reachable outer results. |
| `whole_expression` | `return x + 4` preserves the whole expression, both operand roles, and the reaching source of x. |
| `overwrites_and_copies` | Killed initializers cannot supply the result; a copy made before overwrite retains its old source through unchanged statements. |
| `effective_guards` | Earlier exits, nested branches, short-circuit arms and conditional overwrite order produce exact result-selection conditions. Guard versions survive reassignment. |
| `unknown_completion` | A relevant unresolved call or unsupported control/completion construct cannot be treated as no-op or implicit undefined; unaffected supported regions remain visible. This does not require implementing loops or exceptions in FR001. |
| `scope_and_limits` | Complete return observation retains open caller scope; budget exhaustion marks affected dependency coverage partial. |

## Verification

Run `cargo test --locked --offline --lib ifds_fr001_ -- --list`, then
`cargo test --locked --offline --lib ifds_fr001_` and the
[feature completion gate](README.md#completion-gate).
