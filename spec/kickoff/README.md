# Kickoff: IFDS implementation tasks

Status: planned. These 40 tasks do not implement IFDS or create executable tests.
They cover the [specification](../../docs/ifds/SPEC.md), the complete
[staged roadmap](../../docs/ifds/README.md), and the
[benchmark contract](../../docs/ifds/BENCHMARKS.md).

## How to use these tasks

Implement in dependency order. Independent tasks can proceed in parallel; the
numeric order is a convenient default except for K004, which is intentionally
deferred to stage 11. A roadmap stage is complete only when all its non-deferred
tasks and the earlier applicable gates pass. Later refinements do not excuse
incorrect supported flows or missing uncertainty handling in earlier stages.

Each file defines scope, explicit exclusions, dependencies, named unit tests,
and a verification command. Update its status only after the required tests and
any additional integration gate pass. Keep task IDs stable when adding detail.
Historical bugfix correlation, a UI, automatic root-cause conclusions, and automatic
selection of multiple variables are not part of this kickoff implementation.

## Unit-test convention and completion gate

- Put Rust unit tests beside the implementation under `src/ifds/`, using `#[test]`.
  Test names use the task prefix, for example `ifds_k001_round_trip_report`.
  Every row in a task's test table names a required test suffix and its assertions.
- Tests use in-memory IR/repositories or isolated temporary directories, fake
  metadata providers, clocks, budgets, and schedules as needed. They must not
  access the network, run the analyzed application, or execute its package scripts.
- Run `cargo test --locked --offline --lib ifds_kNNN_ -- --list` first and verify
  that every named test is present and none is ignored. A successful command with
  zero tests is not acceptance. Then run the task's verification command.
- Expected nodes, flows, conditions, endings, and forbidden claims are authored
  independently from the implementation. Do not approve analyzer-generated goldens
  merely because they are stable. New syntax needs positive, negative, changed-flow,
  and unsupported/uncertain-boundary cases, even when not repeated in every row.
- Every implementation task also runs `cargo fmt --check`,
  `cargo test --locked --offline`, and
  `cargo clippy --locked --offline --all-targets -- -D warnings`.
  Existing `tsx_diff` behavior and tests must remain intact.
- Provision pinned dependencies before offline verification. Unit tests are the
  minimum gate, not a replacement for real provider/PR integration tests or measured
  repository-wide precision/performance. Those extra gates are identified below.

The first usable deliverable is K001–K003 plus K005–K013: one scalar variable,
supported straight-line code, full supported upstream/downstream context, precise
deltas, and explicit boundaries. K004 is deliberately deferred until a mature
analyzer and a frozen reviewed real-repository suite exist. Do not wait for later
language features or benchmark percentages to deliver the first milestone.

## Task index

| ID | Task | Roadmap stage |
| --- | --- | --- |
| K001 | [Module contracts and report model](001-contracts.md) | 0 |
| K002 | [Immutable snapshots and query validation](002-snapshots-and-queries.md) | 0 |
| K003 | [Fixture harness and independent oracles](003-test-harness.md) | 0 |
| K004 | [Late benchmark scoring and acceptance gates](004-benchmark-scoring.md) | 11 |
| K005 | [TypeScript parsing and lexical bindings](005-typescript-bindings.md) | 1 |
| K006 | [Straight-line IR lowering](006-straight-line-ir.md) | 1 |
| K007 | [Reaching definitions and origin transfers](007-transfer-functions.md) | 1 |
| K008 | [Single-procedure fixed-point solver](008-intraprocedural-solver.md) | 1 |
| K009 | [Unknown effects and deterministic limits](009-uncertainty-and-limits.md) | 1 |
| K010 | [Full slices and value-lifecycle endings](010-full-flow-slices.md) | 1 |
| K011 | [Cross-revision identity alignment](011-node-alignment.md) | 1 |
| K012 | [Precise before/after deltas](012-flow-comparison.md) | 1 |
| K013 | [Public API, reports, and first milestone](013-analysis-api.md) | 1 |
| K014 | [Branches, early returns, and control influence](014-branches.md) | 2 |
| K015 | [Conditional and short-circuit expressions](015-short-circuit-expressions.md) | 2 |
| K016 | [Loops, break/continue, and cyclic evidence](016-loops.md) | 3 |
| K017 | [TypeScript project metadata and modules](017-project-metadata.md) | 4 |
| K018 | [Matched calls and return summaries](018-interprocedural-calls.md) | 4 |
| K019 | [Recursion and summary evidence](019-recursive-summaries.md) | 4 |
| K020 | [Caller-context downstream continuation](020-caller-continuations.md) | 4 |
| K021 | [Read projections and known spreads](021-read-projections.md) | 5 |
| K022 | [Audited library/source/sink summaries](022-boundary-summaries.md) | 5 |
| K023 | [Exceptions and finally completion](023-exceptions.md) | 6 |
| K024 | [Basic await and promise results](024-basic-await.md) | 6 |
| K025 | [Pinned Bluesky PR #5816 acceptance](025-bluesky-pr-5816.md) | 6 |
| K026 | [Heap places, aliases, and mutation](026-heap-and-aliases.md) | 7 |
| K027 | [Compound and destructuring writes](027-extended-writes.md) | 7 |
| K028 | [Hoisting, TDZ, and parameter forms](028-binding-and-parameter-forms.md) | 8 |
| K029 | [Switch and labeled control transfers](029-switch-and-labels.md) | 8 |
| K030 | [Closures and synchronous function values](030-closures.md) | 8 |
| K031 | [Classes, receivers, and accessor effects](031-classes.md) | 8 |
| K032 | [Bounded path-feasibility refinement](032-path-feasibility.md) | 9 |
| K033 | [Alignment and dispatch refinement](033-identity-refinement.md) | 9 |
| K034 | [Deferred promises, callbacks, and events](034-deferred-execution.md) | 10 |
| K035 | [Generators and iteration protocols](035-generators-and-iterators.md) | 10 |
| K036 | [Audited framework state/effect models](036-framework-models.md) | 10 |
| K037 | [Module initialization and dynamic imports](037-module-execution.md) | 10 |
| K038 | [Summary caching and invalidation](038-caching.md) | 11 |
| K039 | [Demand slicing, parallelism, and measurement](039-scale-and-measurement.md) | 11 |
| K040 | [Coverage registry and release acceptance](040-release-gates.md) | 11 |

## Design traceability

| Requirement | Owning tasks |
| --- | --- |
| Separate language-independent core, single-binding query, immutable snapshots | K001, K002, K005, K017 |
| Finite distributive facts, zero generation, matched calls/returns | K007, K008, K018, K019 |
| Full upstream/downstream graph, unchanged consumers, open lifecycle boundaries | K010, K014, K020, K024, K034 |
| Section 6 cases 1–4 and 8: initializer, overwrite, retarget, source switch, no-op | K011, K012, K013 |
| Section 6 case 5: guard-only change | K014, K015 |
| Section 6 case 6: loop write without losing zero-iteration flow | K016 |
| Section 6 case 7: real helper/consumer change | K021, K022, K025 |
| Section 6 case 9: unknown is not removed/added | K009, K012 |
| Section 6 case 10: unchanged downstream consumer chain | K010, K012, K013, K020 |
| Report evidence, per-direction coverage, grouping, history-ready metadata | K001, K009, K010, K012, K013 |
| Additional TypeScript syntax and execution paths | K014–K037 |
| Independent fixtures, 90% precision, recall/abstention reporting | K003, K004, K025, K040 |
| Budgets, slicing equivalence, cache correctness, measured scale | K009, K038, K039 |

No task may silently narrow the design to changed lines, first consumers, only
edited files, or a fixed number of loop iterations. New syntax/library families
discovered by K040 get additional task files with the same testable format.
