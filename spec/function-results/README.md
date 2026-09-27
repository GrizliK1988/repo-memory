# Function result analysis: implementation tasks

Status: implemented for the first synchronous single-function subset. These
tasks implement the [function result specification](../../docs/ifds/FUNCTION_RESULTS.md) and extend the
[shared analysis contract](../../docs/ifds/SPEC.md).

The first delivery selects one synchronous function and compares all its normal
results under shared inputs. It reuses the existing scalar/branch capabilities.
Users do not need a selected variable. Existing binding analysis keeps its scope
and public contracts.

## Task order

| ID | Task | Dependencies |
| --- | --- | --- |
| FR001 | [Function query and result dependencies](001-function-query-and-slice.md) | K013, K014, K015 |
| FR002 | [Conditions and result comparison](002-result-comparison.md) | FR001, K011, K012 |
| FR003 | [Reports and end-to-end acceptance](003-reports-and-acceptance.md) | FR002, K041 |

K IDs refer to the [kickoff tasks](../kickoff/README.md). Their completed behavior
is the baseline. This subset follows K015/K041 before K016 loops, K018 calls,
or K032 broader predicate reasoning.
The minimal truthiness/result theory in FR002 compares pure guards over paired
symbolic inputs without requiring a declared Boolean entry domain.
FR001 accepts typed entry-assumption expressions; FR002 compares their common
domain and records changes to query scope. Automatic function-rename tracking is
deferred to the [later feature](../function-rename-tracking/README.md). K031 owns
class and object method support.

See [acceptance test examples](TEST_EXAMPLES.md) for source fixtures and expected
semantic claims corresponding to each FR001–FR003 test name. The sketches are
independent acceptance oracles; implemented tests live under `src/ifds/` and `tests/`.

## Completion gate

Use Rust unit-test prefixes `ifds_fr001_`, `ifds_fr002_`, and `ifds_fr003_`.
For each task, run its filter with `-- --list` first and verify every required
suffix exists and is not ignored. Then run its tests and the existing
[shared completion gate](../kickoff/README.md#unit-test-convention-and-completion-gate):
formatting, the full locked offline test suite, and Clippy with warnings denied.
Zero discovered tests do not pass a task. No tests are added by these plans.
The function-result integration suite is `cargo test --locked --offline --test ifds_function_results`.

Author fixture expectations from source semantics before accepting implementation
output. Extend the fixture contract with an explicit function-result target and
independent result-region expectations; preserve existing binding fixtures. Tests
use local immutable inputs without network access or application execution.
Do not report broad precision figures from the small acceptance examples.

## Later capability integration

When K016–K020 become available, add the result-specific loop, call, recursion,
and caller-continuation gates from
[extension boundaries](../../docs/ifds/FUNCTION_RESULTS.md#7-extension-boundaries).
Support for those constructs in binding queries alone does not certify result
comparison. Exceptions, async execution, heap observations, and actual runtime
stack selection remain separately gated capabilities.
