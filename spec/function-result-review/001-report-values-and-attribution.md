# 001 — Show proven values, not only `return` text

Status: confirmed in manually authored examples. No implementation changes were made.

## Problem

In `copy`, the function returns `saved`, captured before the original `x` is overwritten. The full report proves a dependency on literal `1` before the change and `2` afterward. The short report says `saved → saved (Different)`. The relation is correct, but the visible text does not explain why identical-looking expressions differ.

In `priority`, the full report proves that under `a && b`, the last of two writes changes the result from `2` to `1`. The short text says `x → x (Different)`. This also loses the link between the write, its computed value, and the binding read by the return.

In `guard_version`, the same cause is split into two tables because the result changes under `flag` and `!flag` are grouped separately. The tables show complementary swaps that could be explained together.

## Suggested improvement

Show a concise expansion of the computed primitive when the full report proves it, for example `saved (1) → saved (2)` and `x (2) → x (1)`. For computations whose values are not proven, keep the source expressions and the relation (`changed` or `unknown`). Retain references to the return expressions and dependency chains.

Group symmetric regions for one guard change into a single finding: `flag: "yes" → "no"; !flag: "no" → "yes"`. Do not combine regions with distinct evidence unless each remains linked.

## Evidence

In `reports/copy/full.json`, the dependency for returned `saved` leads to literal `1` before and `2` after; the later `x = 9` is not part of the copy's dependency. In `reports/priority/full.json`, only the intersection `a && b` differs. Both compact findings are marked `Different`, so the problem is the explanation, not the established relation.
