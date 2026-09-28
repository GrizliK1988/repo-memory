# 004 — Reduce evidence size without losing information

Status: planned; representation and compatibility decisions require clarification
before implementation.

## Problem

The archived review examples contain compact JSON of about 4–12 KB and full JSON
of about 15–261 KB for small functions. These measurements predate the current
schemas and are not a current baseline or a project performance estimate.
Repeated metadata and dependency nodes are candidates for reducing report size.

Evidence size reduction is separate from the text-only work agreed in
[003](003-compact-readable-text.md#agreed-scope).

## Scope

- Measure freshly generated compact and full reports using the current schemas;
  identify which repeated structures account for their size.
- Evaluate sharing metadata and dependency nodes, then choose a representation
  based on measured savings and the cost of resolving references.
- Preserve complete evidence, reproducibility, deterministic output, and
  independently resolvable compact references into the exact full report.
- Preserve snapshot identity, source positions, result regions, assumptions,
  assessments, proofs, coverage, attribution, and dependency relationships.

Dropping evidence, truncating dependency trees, or changing analysis and
comparison semantics is outside this task. Changes to JSON structures and schema
versions may be considered here; they are not authorized as part of 003.

## Decisions before implementation

- Whether to optimize full JSON, compact JSON, or both, based on the baseline.
- The sharing representation and its reference-validation rules, including how
  distinct observations and snapshots retain their identities.
- Compatibility, schema versioning, reader behavior, and report-ID behavior if
  serialization changes. Archived reports remain intact.
- Representative measurements and the required savings; no numeric target has
  been agreed yet.

## Acceptance criteria

- Record reproducible before/after byte sizes for the same source pairs and
  query options, distinguishing compact output from the full sidecar. Include
  cases with repeated dependencies; label synthetic measurements as such.
- Demonstrate the agreed size reduction without loss of evidence. Verify
  equivalent semantic facts and complete dependency relationships after
  serialization and reading.
- Resolve every compact reference and shared-node reference to its intended
  evidence and snapshot. Invalid or dangling references fail validation.
- Repeated runs produce deterministic reports and IDs under the chosen contract.
- Result assessments, proofs, coverage, and human-readable claims remain
  equivalent for the same queries.
- Document any schema and reader changes; regenerate verification artifacts
  separately from archived reports.

Run the relevant serialization, evidence-validation, function-result unit and
integration checks, and exercise the CLI in all three formats. Record measured
results and the chosen representation before marking the task complete.
