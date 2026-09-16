# K005 — TypeScript parsing and lexical bindings

Status: planned. Roadmap stage: 1.

Dependencies: [K001](001-contracts.md), [K002](002-snapshots-and-queries.md).

Source: [SPEC sections 2–3](../../docs/ifds/SPEC.md).

## Scope

- Select Tree-sitter TypeScript or TSX by file kind; index declarations/references
  for the initial scalar `let`/`const` and parameter subset.
- Resolve the selected declaration by UTF-8 identifier span, not spelling or
  `tsx_diff` qualified-name/occurrence matching. Check any optional declared-name
  and enclosing-symbol assertions against that declaration to detect stale selectors.
  Keep scopes and bindings distinct.
- Diagnose unsupported binding forms, parse failures, and unresolved references.

Excludes project-wide TypeScript resolution, `var`/TDZ semantics, and JSX execution
models; successfully parsing TSX does not make all of its behavior supported.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `grammar_selection` | `.ts` and `.tsx` inputs use the correct grammar; TSX JSX parses but retains a semantic boundary until modeled. |
| `shadowed_names` | Outer local, nested local, and parameter with the same spelling have separate binding IDs and correctly resolved reads. |
| `declaration_not_reference` | A selector on the declaration is accepted; selecting a read occurrence is rejected. |
| `stale_selector_assertions` | A selector whose declared-name or enclosing-symbol assertion disagrees with the resolved declaration is rejected as stale; omitted or matching assertions resolve to the same binding. |
| `unicode_identifier` | Declaration and references map to the exact multibyte spans and one-based lines. |
| `separate_functions` | Two functions' locals named `x` never resolve to each other. |
| `unsupported_or_invalid` | Relevant destructuring/hoisting forms and parser errors produce explicit diagnostics, not guessed bindings. |

## Verification

Run `cargo test --locked --offline --lib ifds_k005_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
