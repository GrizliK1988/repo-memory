# 002 — Preserve equality when `&&` skips its right operand

Status: reproducible loss of precision in supported syntax.

## Example

```typescript
// before
function result(flag) { return flag && "old"; }
// after
function result(flag) { return flag && "new"; }
```

When `flag` is truthy, the report correctly says `"old" → "new" (Different)`. When `flag` is falsy, short circuiting returns the value of `flag` itself in both snapshots. This region is semantically equal.

The current report marks it `Unknown: result value theory is insufficient` and downgrades result comparison coverage to `Partial`. The right operand is not evaluated; both functions return the same paired input. Since short circuiting and the skipped RHS are already analyzed, preserving the identity of the left operand should be possible.

## Acceptance criteria

For falsy `flag`, report an equal result, retaining it as the paired input value or another equivalent proven form. For truthy `flag`, preserve the established difference. Continue to report `unknown` for genuine uncertainty in the value theory or unsupported effects; do not mark overall coverage complete while any region remains unknown.

The original full and compact reports are in [`reports/short_circuit`](reports/short_circuit). The independent truth table and automated comparison are in [`reports/verification.json`](reports/verification.json).
