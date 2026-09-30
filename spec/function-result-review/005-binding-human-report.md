# 005 — Make binding human reports more compact

Status: planned; separate from function-result report task 004.

## Goal and scope

Apply concise human reporting to variable-flow queries targeting a binding.
The user explicitly requested that binding reports be handled separately from
[004](004-evidence-size.md), which changes only function-result text.

Inspect freshly generated binding reports before choosing specific presentation
rules. Identify repeated explanations and source links and distinguish essential
conclusions, scope, and uncertainty from evidence details. Do not assume function
return-choice tables or result-equality claims apply to binding flow reports.

Use 004's presentation direction: concise human text by default, detailed text
on request, and full evidence by reference. Preserve JSON contracts, analysis/
comparison semantics, snapshot identity, report identity, and reference validation.
Inspect coupling between binding human summaries and report IDs before changing
rendering; do not silently change IDs or evidence content for the new text mode.
The [004 verification](004-implementation.md#measurements-and-parity) confirmed that
binding `stats.elapsed_ms` already affects IDs across independent CLI executions;
account for this existing limitation when measuring parity.

## Decisions specific to this task

- Representative binding queries and reproducible current text reports.
- Which flow relationships, changes, conditions, and source positions must remain
  in concise text, and which details belong in detailed output/evidence.
- Concrete short/detailed examples preserving coverage, uncertainty, query scope,
  limits, and omission notices.
- How to extend 004's text-mode option to binding targets while preserving their
  evidence and identity contracts.

## Acceptance criteria

- Record agreed before/after binding examples and demonstrate less repetition
  without hiding material flow changes or uncertainty.
- Show evidence-backed relationships and source positions, preserving snapshot
  distinctions and complete retrievable evidence.
- Verify deterministic text for a fixed structured report, short/detailed
  semantic parity, structured-output
  parity, and valid references to the exact full report for identical queries.
- Keep function-result behavior from 004 unchanged.
- Document CLI behavior and run focused binding renderer, evidence-validation,
  and CLI checks. Preserve archives and save fresh verification separately.
