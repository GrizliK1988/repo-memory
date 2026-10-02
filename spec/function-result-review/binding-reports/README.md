# Fresh binding reports for 005

These reports were generated during [005 verification](../005-verification.json),
using binding `x`, containing-function scope, and the CLI's default limits.
They are archived observations, not semantic golden expectations. Source-based
Rust assertions supply the independent semantic checks. Earlier function-result
archives and 004 artifacts remain untouched.

Each case directory contains:

- `before.ts` and `after.ts`: the exact input snapshots; the CLI exposes their
  locations under the canonical path `snippet.ts`.
- `short.txt` and `full.json`: current default text and its exact full sidecar.
- `verbose.txt` and `verbose-full.json`: detailed text and its exact full sidecar.
- `compact.json` and `compact-full.json`: structured projection and its exact evidence.
- `legacy.txt` and `legacy-full.json`: pre-005 human text and its exact baseline evidence.

Evidence paths in the text and JSON are relative to the repository root.
The full files belong to independent CLI executions: existing `stats.elapsed_ms`
can change their IDs, even with identical source pairs and queries. Never resolve
one mode's ID against another mode's sidecar merely because its source pair matches.
Fixed-report rendering uses one full object for both modes and is covered by Rust
round-trip, identity, and canonical-byte parity checks.

Regenerate verification in a new empty directory with `verify_005.py`; archived
report directories are never overwritten by that script. See the
[implementation review](../005-implementation.md) for measurements and reproduction.
