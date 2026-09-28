# 002 — Preserve paired input identity in function results

Status: implemented and verified against the clarified acceptance criteria.

## Example

```typescript
// before
function result(flag) { return flag && "old"; }
// after
function result(flag) { return flag && "new"; }
```

When `flag` is truthy, the report correctly says `"old" → "new" (Different)`. When `flag` is falsy, short circuiting returns the value of `flag` itself in both snapshots. This region is semantically equal.

The archived report marked it `Unknown: result value theory is insufficient` and downgraded result comparison coverage to `Partial`. The right operand is not evaluated; both functions return the same paired input. The implementation now preserves that identity and proves equality in the skipped region.

## Acceptance criteria

### General input identity rule

Compare both versions on the same paired inputs. If both normally completing results are proven to return the unchanged value of the same paired positional input, assess the region as `Equal` without requiring a declared primitive type or evaluating that value. Apply this rule to direct returns, copies, and supported operand selection, rather than adding an exception only for the falsy arm of `&&`.

For example, `return flag` and `const copy = flag; return copy` are equal on the same paired input even without an input type declaration. Input identity follows the established input correspondence and actual value dependencies; matching variable names or expression text alone is insufficient. Parameter renaming does not invalidate established correspondence. A reassignment must be followed to the value actually returned.

This intentionally extends the original primitive-input-only identity rule. [FR002](../function-results/002-result-comparison.md) and [the function result specification](../../docs/ifds/FUNCTION_RESULTS.md) now record the general rule. It proves identity of the same supplied value, not equality of independently supplied objects or equivalence of arbitrary computations. Existing conservative rules for calculations, coercions, unsupported effects, missing input correspondence, and distinct input selection continue to apply.

### Supported operand selection

Apply the identity rule to `&&`, `||`, and `??`, including copies and nested expressions wherever the existing analysis can prove which operand is selected:

- `&&` returns its left operand when that value is falsy.
- `||` returns its left operand when that value is truthy.
- `??` returns its left operand when that value is neither `null` nor `undefined`.

Do not broaden predicate solving merely to make every nested expression or nullish condition decidable. Unresolved selections remain `Unknown`. A truthiness condition is not a strict comparison with Boolean `true` or `false`.

For the original example, retain two proven regions: falsy `flag` returns the same input and is `Equal`; truthy `flag` returns `"old"` before and `"new"` after and is `Different`. Result comparison coverage becomes `Complete`. Source alignment coverage remains independently assessed.

### Presentation and structured evidence

Omit ordinary proven `Equal` regions from the text output. [003](003-compact-readable-text.md) adds a concise exception for established control changes with equal results and a scoped summary for fully equal results; it does not restore ordinary equal-region listings. For the original example, the effect should communicate only:

```text
when flag is truthy: "old" -> "new" (Different)
```

Use `truthy`, not `is true`, for an unrestricted input. The example defines the visible condition and result meaning; it does not require changing unrelated text formatting or evidence lines.

Keep equal regions, their input identity proofs, and coverage in both compact and full JSON. Preserve original source expressions, source references, and proof references in the structured reports. Hiding an equal region in text is not loss of comparison coverage. Unknown regions and their reasons remain visible in text and JSON.

### Skipped unsupported calls

The following comparison is an obligatory acceptance case:

```typescript
// before
function result(flag) { return flag && "old"; }
// after
function result(flag) { return flag && mystery(); }
```

For falsy `flag`, prove that the call is skipped and both results return the same input. Assess this region as `Equal`, retain it in JSON, and omit it from text. For truthy `flag`, the call's returned value or normal completion is unresolved: report `Unknown` with its reason. Result comparison coverage remains `Partial`.

An unresolved call or effect that actually executes before the return still prevents an equality claim for the affected region, even if continuing paths return the same input. Continue to report `Unknown` for genuine uncertainty; never mark result comparison coverage complete while any feasible comparison region remains unknown.

### Verification boundaries

Verify the original example, direct input returns versus copies without type declarations, all three operators where selection is supported, and copies and nested expressions. Include parameter renaming, reassigned values, and distinct input selections as checks that equality follows actual paired value identity. Verify skipped unsupported calls separately from calls that execute. Check text omission of equal regions together with their preservation and evidence references in compact and full JSON.

The original full and compact reports are in [`reports/short_circuit`](reports/short_circuit). The independent truth table and automated comparison are in [`reports/verification.json`](reports/verification.json).

Keep these original reports archived. Generate fresh reports for implementation verification rather than overwriting the pre-implementation evidence.

## Implementation verification

The comparison regressions are `ifds_fr002_untyped_paired_input_identity`,
`ifds_fr002_short_circuit_input_identity`, `ifds_fr002_nullish_operand_selection`,
and `ifds_fr002_skipped_call_input_identity`. Reporting is checked by
`ifds_fr003_short_circuit_identity_report` and the integration test
`short_circuit_identity_reports_and_skipped_calls`.

Verification passed with the full locked offline test suite,
`cargo fmt --all -- --check`, and
`cargo clippy --locked --offline --all-targets -- -D warnings`.
The original short-circuit source pair was rerun through the CLI in text and
compact JSON formats using a fresh full evidence sidecar. The falsy region retains
`paired_input_identity`, the truthy region is `Different`, and result comparison
coverage is `Complete`. The text effect is the example shown above.

Nullish selection is proven for known supported values and declared primitive
domains. With an unrestricted input, its truthy region is known to be non-nullish;
the falsy region remains `Unknown` because it can include both nullish and
non-nullish values. No general nullish predicate solver was added.
