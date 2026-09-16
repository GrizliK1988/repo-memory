# K006 — Straight-line IR lowering

Status: planned. Roadmap stage: 1.

Dependencies: [K005](005-typescript-bindings.md).

Source: [SPEC section 4, IR operations](../../docs/ifds/SPEC.md).

## Scope

- Lower supported initialization, scalar reassignment, copies, literals, primitive
  expressions, templates, reads, and ordinary returns into ordered operations.
- Capture RHS reads in temporaries before assignment commit; attach source spans,
  binding IDs, expression/input roles, and entry/exit structure.
- Define the exact primitive subset. Coercions requiring unknown user code and
  unsupported constructs become `UnknownEffect`, not harmless skipped syntax.

Excludes branches, loops, and executing calls.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `initializer_order` | `let x = input + 1` lowers input read and calculation before the write to `x`. |
| `self_assignment_order` | `x = x + 1` reads the old binding into a temporary before committing the new write. |
| `multiple_operands` | `let y = a + b` has separate input slots; a template retains each interpolation's provenance. |
| `return_ends_path` | `return x; x = 2` has no normal edge from return to the later write. |
| `source_mapping` | Every lowered read/write/compute maps to the correct source construct and byte span. |
| `unsupported_effect` | A relevant call or unknown coercion produces an effect boundary rather than an invented constant or identity transfer. |

## Verification

Run `cargo test --locked --offline --lib ifds_k006_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
