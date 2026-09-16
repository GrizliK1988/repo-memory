# K031 — Classes, receivers, and accessor effects

Status: planned. Roadmap stage: 8.

Dependencies: [K030](030-closures.md).

Source: [roadmap stage 8](../../docs/ifds/README.md).

## Scope

- Model constructors, supported field initialization, instance/static locations,
  method receivers, inheritance, and resolved getter/setter effects.
- Resolve call targets using compatible runtime/metadata evidence, not signatures
  alone. Unknown overriding/accessor behavior remains an effect boundary.
- Carry property and captured origins through matched receiver-aware calls.

Excludes declaring arbitrary dynamic dispatch precise just because types compile.

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `constructor_field_order` | Supported constructor/field initialization writes occur in reviewed order with correct allocation/receiver identity. |
| `instance_static_separation` | Different instances and static fields do not share property writers without alias evidence. |
| `method_receiver` | A resolved method's this.field reads/writes map to the actual receiver's tracked place. |
| `inheritance_dispatch` | Proven base/override/super targets use the correct body and return site; unresolved overrides remain unknown. |
| `accessor_effects` | A resolved getter/setter may execute writes and throws; it is not lowered as an unconditional pure field read/write. |
| `arrow_receiver_retained` | An arrow defined in a method keeps that lexical receiver when invoked elsewhere. |
| `method_change_delta` | Editing a resolved method's value source updates unchanged caller inputs without class-wide fictitious flow changes. |

## Verification

Run `cargo test --locked --offline --lib ifds_k031_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
