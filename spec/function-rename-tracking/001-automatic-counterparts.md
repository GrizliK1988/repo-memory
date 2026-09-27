# FRT001 — Automatic renamed-function counterparts

Status: planned post-kickoff feature.

Dependencies: [FR003](../function-results/003-reports-and-acceptance.md),
[K033](../kickoff/033-identity-refinement.md).

Source: [function result query and input contract](../../docs/ifds/FUNCTION_RESULTS.md#2-query-inputs-and-compatibility).

## Scope

- Infer a renamed function's counterpart across validated snapshots using file
  rename metadata, enclosing-owner identity, declaration structure, and
  independently aligned body evidence. Preserve exact source spans and provenance
  for the chosen match.
- Require one uniquely justified candidate. Keep candidate sets and matching
  coverage unresolved when evidence is ambiguous or insufficient; do not infer a
  match from names, source order, line similarity, or TypeScript types alone.
- Let an explicit valid counterpart override automatic inference. Reject a stale
  or contradictory explicit selector rather than silently replacing it.
- Distinguish a proven addition/removal from a possible rename before making a
  function-presence finding. Keep established result regions independent of an
  unrelated ambiguous function.

Excludes historical identity tracking across more than the selected snapshot
pair and automatic selection of which function the user intended to analyze.

## Required unit tests

| Test suffix | Required evidence |
| --- | --- |
| `unique_rename` | A function renamed in a validated file maps to one structurally supported counterpart and retains before/after result observations. |
| `ambiguous_rename` | Two equally plausible candidates remain unresolved; neither is chosen by name, source order, or text similarity alone. |
| `explicit_counterpart` | A valid explicit selector wins; stale or contradictory selectors fail validation. |
| `presence_vs_rename` | Only independently established absence yields an addition/removal finding; a plausible unresolved rename does not. |
| `stable_unaffected_results` | Refining one function match does not alter unrelated proven result regions or evidence identities. |

## Verification

Run `cargo test --locked --offline --lib ifds_frt001_ -- --list`, verify every
required suffix is present and enabled, then run that filter and the
[function-result completion gate](../function-results/README.md#completion-gate).
