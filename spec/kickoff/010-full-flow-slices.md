# K010 — Full slices and value-lifecycle endings

Status: done. Roadmap stage: 1.

Dependencies: [K008](008-intraprocedural-solver.md), [K009](009-uncertainty-and-limits.md).

Source: [SPEC sections 5 and 6 case 10](../../docs/ifds/SPEC.md).

## Scope

- Build the union of selected writes, upstream inputs, and complete supported
  downstream dependencies; retain unchanged intermediate/final consumers.
- Follow origins through copies independently of the original binding's lifetime.
  Record per-origin/carrier endings, coverage, witnesses, and exact consumer slots.
- Support straight-line no-further-use, entry exit, escaping return, and unknown
  boundaries now; later tasks extend the same contract to controls/cycles/calls.
- Expose source-level consumer operations in the report graph. Lowered reads and
  temporaries may remain in witnesses, but must not split the public A-to-B-to-D-to-R
  chain into adapter-specific implementation nodes. Consumer-slot projections are
  part of relation identity.
- Treat endings per origin and carrier. An overwrite ends that carrier at the
  overwrite location without closing surviving copies. Place `no_further_use` at
  the last supported use, at the overwrite that kills the last carrier, or at the
  originating write when the value is never read. A returned tracked value has a
  `scope_boundary` unless its caller context is explicitly included.
- At an unsupported operation, retain the supported prefix and add an
  `unknown_boundary` for each affected continuation. Do not infer dependencies
  through unknown results or effects; continue facts proven unaffected and
  independent supported carriers. Any reachable unknown boundary keeps that
  origin's lifecycle open.

Excludes including unrelated later statements or mining historical bugfixes.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `complete_copy_chain` | SPEC case 10 retains the source-level A-to-B-to-D-to-R chain despite unchanged consumers and C's overwrite of `x`; C remains a selected write but does not feed D/R. Internal read/temporary nodes may support witnesses but do not replace these public consumer nodes. |
| `one_copy_survives` | Killing carrier `x` records its ending at that overwrite but does not close the same origin's surviving copied carrier or the origin lifecycle. |
| `last_carrier_ends` | Cover a final carrier killed by overwrite, a last supported use, and a never-read selected write; record `no_further_use` at the overwrite, last use, and originating write respectively. |
| `returned_value_escapes` | A tracked entry-function return records `scope_boundary`, not lifecycle closure; an entry exit with no escape is distinguished. |
| `no_sibling_expansion` | An independent use of an upstream input shared by a calculation is not included as a downstream use of selected `x`. |
| `unknown_is_open` | A relevant input to an unsupported operation retains its known argument/input relation and records `unknown_boundary`, but no relation through an unknown result. A proven-unaffected carrier and an independent branch continue; lifecycle closure remains false. |
| `sliced_equals_unsliced` | On supported small programs, the production slice equals the selected slice extracted from a full reference analysis: selected writes (including unused writes), normalized source-level nodes, typed relations and projections, endings, coverage, completeness, and lifecycle closure match. Internal temporaries, witness IDs/choice, traversal order, and statistics need not match. The full reference path is test-only, not a production fallback. |

## Verification

Run `cargo test --locked --offline --lib ifds_k010_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
