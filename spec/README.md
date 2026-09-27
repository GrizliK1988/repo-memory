# Feature task sets

Implementation work is grouped by feature under `spec/<feature>/`. Each folder has
an ordered task index and one Markdown file per independently verifiable task.

- [Kickoff: IFDS variable-flow analysis](kickoff/README.md): the current analysis
  specification, decomposed into implementation tasks and unit-test acceptance sets.
- [Function result analysis](function-results/README.md): planned function targets,
  return dependencies, same-input condition/result comparison, and report acceptance.

Add later features in sibling folders rather than mixing their tasks into kickoff.
Task descriptions are implementation plans, not evidence that code or tests exist.
The design documents in [docs/ifds](../docs/ifds/README.md) remain the semantic
source of truth; these files specify how to deliver and verify that design.
