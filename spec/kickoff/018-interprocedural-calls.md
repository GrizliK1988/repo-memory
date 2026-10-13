# K018 — Matched calls and return summaries

Status: done. Roadmap stage: 4.

Dependencies: [K016](016-loops.md), [K017](017-project-metadata.md).

Source: [SPEC section 4, procedures and solver](../../docs/ifds/SPEC.md).

## Scope

- Lower resolved direct calls with distinct call/return-site IDs. Map argument
  facts to formal parameters, return origins to result temporaries, and preserve
  only proven unaffected caller facts on bypass edges.
- Implement IFDS entry-to-node records, waiting callers, and reusable entry/exit
  summaries. Apply the normal write rule when assigning a call result.
- Attach call context to all returned-flow witnesses, not only internal facts.

Excludes recursion-specific completion tests and unmodeled captured/heap effects.

## Agreed implementation contract

- Connect both variable-flow and function-result metadata APIs; APIs without
  metadata retain unknown-call behavior.
- Qualify file/call identities and attach explicit call context to evidence.
  Raise public report schema versions where needed.
- Use reusable IFDS summaries, then bounded nonrecursive evidence expansion for
  the existing branch validation/report pipeline. Recursive report evidence is
  an explicit K019 boundary; included caller entries stay in K020.

Implementation details: [matched calls](../../docs/ifds/INTERPROCEDURAL_CALLS.md).

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `argument_return_chain` | A resolved identity/transform helper carries actual origins into its parameter and back to the correct result temporary. |
| `two_calls_no_leak` | SPEC's decorate calls A(language) and B("de") never transfer A's origin to B's result or witness. |
| `scalar_parameter_reassignment` | Reassigning a parameter does not rewrite the caller binding; assigning the returned result is a separate caller write. |
| `unaffected_caller_locals` | A known unrelated call preserves caller locals; only documented call effects invalidate facts. |
| `summary_reuse` | Repeated compatible entry facts reuse summaries while preserving distinct callers and result places. |
| `zero_and_new_sources` | Zero reaches a callee that returns a fresh literal even without argument origins. |
| `unknown_target` | An unresolved dispatch has an open boundary and does not inherit known-callee purity or return facts. |

## Verification

Run `cargo test --locked --offline --lib ifds_k018_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).

Also run `cargo test --locked --offline --test ifds_calls` for both public metadata
APIs against the pinned TypeScript provider, helper-body changes, immutable local
function initializers and unknown dispatch.

Completed verification: all 16 K018 unit tests and both integration tests pass.
The full offline suite passes all 335 tests with no failures or ignored tests;
`cargo fmt --check`, `git diff --check`, and Clippy for all targets with warnings
denied also pass.
