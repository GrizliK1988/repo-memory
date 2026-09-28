# 001 — Show proven values, not only `return` text

Status: implemented and verified. See the [implementation review and regenerated text](001-implementation.md). The evidence reports linked below remain archived observations of the earlier implementation.

## Problem

In `copy`, the function returns `saved`, captured before the original `x` is overwritten. The full report proves a dependency on literal `1` before the change and `2` afterward. The short report says `saved → saved (Different)`. The relation is correct, but the visible text does not explain why identical-looking expressions differ.

In `priority`, the full report proves that under `a && b`, the last of two writes changes the result from `2` to `1`. The short text says `x → x (Different)`. This also loses the link between the write, its computed value, and the binding read by the return.

In `guard_version`, the same cause is split into two tables because the result changes under `flag` and `!flag` are grouped separately. The tables show complementary swaps that could be explained together.

## Agreed scope

Implement the improvement in text and compact JSON. Both must derive from the same structured facts. Preserve existing result assessments; derive the additional presentation facts from full observations, entry assumptions, dependencies, and comparison regions. Include source positions on full dependency nodes so compact links to contributing assignments can be checked against the evidence.

This task owns primitive-value annotations, region-aware return choices, concise links to contributing definitions, and grouping symmetric effects of an established common guard change. The remaining readability and coverage-status work belongs to [003](003-compact-readable-text.md).

## Proven values

Show a concise expansion of a proven primitive next to the original return expression: `saved (1) → saved (2)` and `x (2) → x (1)`. Preserve the expression's source location and the evidence establishing the value on each side.

For this task, a value is proven when the existing supported result evidence resolves the whole returned value to an ordinary primitive literal, including through copies, reaching definitions, or an explicitly known input. Use the existing primitive-value model: Boolean, number, string, null, and undefined. Apply the existing restrictions on unsupported special numeric values.

The value must hold throughout its stated feasible input region under the recorded entry assumptions. Inspect the dependency of the selected return and the binding version it reads. A literal somewhere inside a dependency tree is insufficient. In particular, `input + 1` with a known `input = 4` is a computation, not a returned `4`.

Arithmetic evaluation and broader expression evaluation are outside this task. `known_expression` continues to show `input + 1 → input + 2 (Changed)`, without `5 → 6`. TypeScript annotations and truthy/falsy regions do not supply concrete input values: truthy `flag` is not necessarily `true`, and falsy `flag` is not necessarily `false`.

Derive the optional value independently for each side. When it is not proven, retain the expression without a value annotation. Keep `changed` and `unknown` assessments and their reasons as established by comparison. Unknown dependencies or uncertain normal completion must not become a proven returned value merely because a literal is reachable in the dependency tree. Preserve existing analysis and comparison coverage.

## Region-aware presentation

Use the proven value in effect statements, return-choice tables, and equal-result lines. Keep the original expression; display strings with quotes and proper escaping. Avoid redundant annotations on direct literals, bare returns, or implicit undefined: display `1`, `"yes"`, or `undefined`, rather than `1 (1)` or `undefined (undefined)`.

Choice identity and before/after selection conditions must account for the returned value and its evidence, rather than only the source text. The same expression `x` can represent different values in different regions. Do not present `x` as a single unchanged choice when its value changes. Split regions when a single annotation does not hold throughout them, and keep complete selection conditions, common contexts, and relative effects consistent with the full evidence.

For `priority`, the affected choices are:

| Return choice | Before | After |
| --- | --- | --- |
| `x (1)` | `a && !b` | `a` |
| `x (2)` | `b` | `!a && b` |

The effect is `a && b: x (2) → x (1) (Different)`. Other input regions remain equal: `!a && !b` returns `x (0)`, `a && !b` returns `x (1)`, and `!a && b` returns `x (2)`. A factored or logically equivalent presentation is acceptable if it preserves these complete conditions. The unchanged `x (0)` choice follows the existing unchanged-choice policy.

## Attribution and evidence

Add a concise explanation linking the return expression to its contributing definition, with locations in the corresponding before/after source. For `copy`, the explanation connects `return saved` to `const saved = x` and the original `x = 1` or `x = 2`. Exclude the later `x = 9`, which does not supply the saved value. For `priority`, identify the last applicable assignment in each snapshot under `a && b`.

Keep the complete dependency chain in the full evidence sidecar. Compact JSON must retain resolvable links to the return observation, the contributing dependency nodes, and the comparison region. Text shows a short explanation with source locations and a link to the sidecar; it need not print the entire tree.

Distinguish an established value dependency from attribution to a specific source edit. If a unique contributing edit cannot be established, preserve that uncertainty and show only the supported dependencies. A proven value does not imply certain edit attribution.

## Symmetric guard effects

Group complementary regions into one compact finding when evidence establishes that they arise from the same guard change. For `guard_version`, show one return-choice table and both effects:

```text
flag: "yes" → "no" (Different)
!flag: "no" → "yes" (Different)
```

Each effect retains its own input region, before/after result, assessment, and evidence references. Combining the presentation must not replace these two directional effects with one scalar before/after pair or remove their separate full comparison regions.

Matching result labels or identical choice tables alone do not establish a common cause. Verify the correspondence of the guard and its contributing value versions across snapshots. If that correspondence is unresolved, keep the findings separate. Preserve enclosing contexts; do not merge independent changes merely because their regions are complementary.

## Compact JSON contract

Store source expressions and optional typed proven values separately. Reuse `FunctionKnownValue` for the value representation; do not encode a number, Boolean, null, or undefined solely inside a formatted display string. An absent proven value means it was not established, distinct from a proven null or undefined.

Represent a grouped finding's effects structurally, associating each region with its own before/after results and evidence. Source locations and dependency references must preserve snapshot identity. Text formatting must consume these same facts.

Bump the compact function-result schema version for the changed structure and update its serialization and validation together. Source positions require extending the full dependency structure: the full report version becomes 2, the analysis version 4, and the compact version 2. Keep binding-report schemas unchanged. Validate the new compact facts against the full evidence, including proven values, selection conditions, grouping, attribution, and every reference. This task does not require migration of archived reports.

## Acceptance criteria

| Case | Required behavior |
| --- | --- |
| `copy` | Show `saved (1) → saved (2) (Different)` for all inputs. Link both returns, the saved copy, and the original assignments; exclude the later overwrite. |
| `priority` | Show `x (2) → x (1) (Different)` only under `a && b`; preserve the complete choice conditions above and proven equal values elsewhere. |
| `guard_version` | One compact finding and one choice table contain both directional effects, backed by established common-guard evidence and separate region references. |
| Independent guard changes | Keep separate findings when a shared cause is absent or unresolved, even with matching labels or complementary regions. |
| `literal`, `fallthrough` | Preserve direct literal and undefined presentation without duplicate value annotations. |
| `overwritten` | The returned value is `x (9)` on both sides; changed overwritten assignments do not become contributing causes. |
| Explicitly known input | A direct return or copy may expose the known primitive under the recorded assumptions; a type annotation or truthiness constraint alone does not supply it. |
| `expression`, `known_expression` | Preserve the existing `Unknown` / `Changed` assessments respectively; do not evaluate arithmetic or annotate an operand's value as the whole result. |
| One-sided proven value | Annotate the proven side only and retain the other expression and established relation, without inventing a value. |
| `unknown` | Keep the independent known difference and unresolved completion region visible; preserve partial comparison coverage. |
| Text/JSON/evidence parity | Render identical value, condition, and grouping claims from compact facts; typed values round-trip and resolve to the full evidence. Invalid values or references fail validation. |
| Determinism and limits | Grouping and ordering are deterministic. Presentation limits preserve retrievable evidence and report omitted groups without implying complete or unchanged behavior. |

Use the existing source examples as independent semantic oracles. Add focused reporting tests, including negative grouping and incorrect-value validation cases, and run the relevant function-result unit and integration checks. Regenerate example reports for review when implementing; preserve the provenance of the archived reports.

## Evidence

In `reports/copy/full.json`, the dependency for returned `saved` leads to literal `1` before and `2` after; the later `x = 9` is not part of the copy's dependency. In `reports/priority/full.json`, only the intersection `a && b` differs. Both compact findings are marked `Different`, so the problem is the explanation, not the established relation.
