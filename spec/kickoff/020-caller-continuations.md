# K020 — Caller-context downstream continuation

Status: planned. Roadmap stage: 4.

Dependencies: [K019](019-recursive-summaries.md).

Source: [SPEC sections 1, 3 and 5; case 10](../../docs/ifds/SPEC.md).

## Scope

- Support a specific call's known arguments and explicitly included enclosing
  caller context while keeping exactly one selected binding inside the callee.
- Follow selected origins through return sites into unchanged caller consumers,
  nested calls, and other files; preserve the outermost declared execution boundary.
- Report default-entry escaping values as open scope boundaries, never silently
  merge all application callers to extend the graph.

Excludes automatic whole-repository entry discovery or history correlation.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `default_vs_known_input` | chooseLanguage with unknown inputs retains both branches; known ("de", false) excludes the fallback writer. |
| `return_into_caller` | Selecting a callee local and including its caller preserves the path into the caller's later calculation/return. |
| `default_scope_boundary` | Without included caller context the same return ends at `scope_boundary`, not `no_further_use`. |
| `unchanged_other_file` | A downstream consumer outside edited files remains in the graph and receives changed provenance where supported. |
| `separate_callers` | Selecting one call context adds no origin or consumer path from another invocation. |
| `outermost_boundary` | Nested included calls continue through matched returns and stop explicitly at the declared outer scope. |

## Verification

Run `cargo test --locked --offline --lib ifds_k020_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
