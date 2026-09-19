# K006 — Straight-line IR lowering

Status: done. Roadmap stage: 1.

Dependencies: [K005](005-typescript-bindings.md).

Source: [SPEC section 4, IR operations](../../docs/ifds/SPEC.md).

## Scope

- Lower supported initialization, scalar reassignment, copies, literals, primitive
  expressions, templates, reads, and ordinary returns into ordered operations.
- Capture RHS reads in temporaries before assignment commit; attach source spans,
  binding IDs, expression/input roles, and entry/exit structure.
- Treat type correctness as an upstream TypeScript-service responsibility. This
  crate models data dependencies and does not reproduce TypeScript type checking
  or JavaScript coercion semantics.
- Return a valid partial IR with explicit diagnostics and `UnknownEffect` nodes for
  valid but unsupported syntax. Structural/parser/invariant failures return an error.

Excludes branches, loops, and executing calls.

## Kickoff language subset

The initial primitive subset contains number, string, boolean, null, bigint, and
undefined literals; identifier reads; parentheses and runtime-transparent TypeScript
wrappers; binary `+`, `-`, `*`, `/`, `%`, `===`, and `!==`; unary `+`, `-`, and `!`;
ordinary templates; pure property/index projections; scalar declarations and `=`
assignments; and ordinary returns. Operator support is syntax-based, not gated on
locally inferred operand types.

Multiple scalar declarators lower left-to-right. `let x` writes `undefined`.
Destructuring, compound/update/nested assignments, object/array/function literals,
optional chaining, heap writes, tagged templates, and suspension remain explicit
boundaries. Property and index reads are deliberately treated as pure projections
in this stage; getters and proxy traps are deferred to a post-kickoff cycle.

Short-circuit/conditional expressions belong to K015. Branches, loops, switches,
and other unsupported control constructs produce a control-effect frontier with no
invented normal successor. Value effects (including unresolved calls) may have an
unknown result and retain normal continuation. Calls evaluate the callee and
arguments in language order before the boundary.

## IR and ordering decisions

- Build the complete IR for the selected binding's containing procedure, then let
  analysis/reporting select relevant upstream and downstream operations. Nested
  function bodies are separate procedures and closure capture is an explicit boundary.
- A top-level binding uses a synthetic module procedure. Function declarations,
  expressions, arrows, methods, constructors, getters, and setters are procedure
  boundaries. An expression-bodied arrow has an implicit `Return` operation.
- Each source occurrence is a distinct `Read` and temporary. `Compute.inputs` is an
  ordered `Vec<ComputeInput>` with operand/interpolation/chunk roles and spans; it
  preserves repeated operands.
- `Literal`, `Compute`, `Write`, and a result-producing `UnknownEffect` own static
  `DefinitionId`s. A `Read` copies existing facts and does not create a definition.
  A produced temporary is identified by its producer's `NodeId`.
- Literal IR stores its kind and raw spelling rather than an `f64`; template chunks
  may additionally store a cooked value. Primitive operators use a typed enum.
- IDs are deterministic. Procedures are numbered in source order (the module is
  first), and operation IDs encode `(procedure ordinal, operation ordinal)`. K011,
  not K006, aligns IDs across snapshots.
- `return expr` evaluates the expression before a source-mapped `Return`, and every
  explicit return reaches one shared synthetic normal `Exit`. Fallthrough reaches
  that `Exit` directly. Source operations after an unconditional return remain as
  an unreachable subgraph and receive no edge from the return.

## Source mapping and API

Reads and literals use their exact token spans; computations use the whole expression;
declaration writes use the individual declarator; assignment writes use the whole
assignment expression; returns and unknown effects use the whole source construct.
Synthetic entry/exit nodes have no span. Straight-line normal edges need no construct
span.

The adapter entry point accepts source text, a `TypeScriptBindingIndex`, and an already
resolved `BindingId`, and returns the containing `ProcedureIr`, selected binding, and
sorted diagnostics. It validates the IR before returning success. A missing selected
binding, parse failure, snapshot mismatch, or invalid IR is an error; unsupported but
structurally valid syntax is not.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `initializer_order` | `let x = input + 1` lowers input read and calculation before the write to `x`. |
| `self_assignment_order` | `x = x + 1` reads the old binding into a temporary before committing the new write. |
| `multiple_operands` | `let y = a + b` has separate input slots; a template retains each interpolation's provenance. |
| `return_ends_path` | `return x; x = 2` has no normal edge from return to the later write. |
| `source_mapping` | Every lowered read/write/compute maps to the correct source construct and byte span. |
| `unsupported_effect` | A relevant call produces a value-effect boundary rather than an invented constant or identity transfer. |
| `control_frontier` | Unsupported control produces an explicit control-effect frontier with no invented normal successor. |

## Verification

Run `cargo test --locked --offline --lib ifds_k006_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
