# K017 — TypeScript project metadata and modules

Status: done. Roadmap stage: 4 (other stage 4 tasks remain planned).

Dependencies: [K013](013-analysis-api.md).

Source: [SPEC section 2](../../docs/ifds/SPEC.md).

## Scope

- Define a versioned JSON metadata protocol and pinned Node/TypeScript compiler-API
  bridge for declaration identities, imports/re-exports, signatures, and resolution.
- Honor snapshot tsconfig, aliases, extensions, and runtime/module configuration.
  Keep the Rust solver independent of the provider; fake responses support unit tests.
- Missing/excluded bodies and unavailable metadata become explicit boundaries.
  Do not run application code, package scripts, or custom compiler plugins.

Excludes assuming declaration-only types prove purity or runtime dispatch.

## Agreed implementation contract

- Use the direct TypeScript Compiler API through a small Node bridge with a pinned
  compiler. Node is a prerequisite; analysis never installs dependencies.
- First real acceptance case: pinned Bluesky web sources and internal aliases.
  Workspace-package bodies are outside this initial reference case.
- Operate on immutable source/config data in a virtual file system. Explicit
  tsconfig selection wins; automatic discovery requires an unambiguous project.
- Missing root/base configs are structured input errors. Callers (including an
  agent) must supply them explicitly per snapshot. There is no guessed default.
- External-package calls retain unknown behavior. Missing internal/generated files
  and excluded bodies are distinct named boundaries. Type-check success is not
  required; this task is not a type checker.
- JSON records full resolution chains, exact source spans and IR mappings. Text
  shows targets/statuses; verbose text shows chains. Preserve existing scalar
  unknown-call behavior until K018 provides argument/return transfers.
- Keep declared runtime mappings and configuration provenance in the report.
  Type-only imports, signatures, overloads and mutable function bindings must not
  fabricate executable targets. Unsupported IR mapping remains explicit.

Implementation details and CLI/API examples:
[project metadata](../../docs/ifds/PROJECT_METADATA.md).

## Required unit tests

| Test suffix | Input and exact assertions |
| --- | --- |
| `protocol_versions` | Valid protocol/compiler versions decode; unsupported versions fail without fabricated symbols. |
| `aliases_and_reexports` | A small project metadata fixture resolves an alias through a re-export to the intended declaration. |
| `local_shadowing` | A local declaration hides an imported same-named function at the correct call site. |
| `config_dependent_target` | Two explicit module configurations produce their reviewed targets; incompatible runtime candidates remain unresolved. |
| `declarations_not_effects` | A `.d.ts` signature without a body/model yields no purity guarantee or invented return origin. |
| `provider_failure` | Missing provider/body or excluded dependency produces the matching diagnostic and preserves unaffected analysis. |
| `safe_provider_request` | Recorded requests contain pinned snapshot/config data and no application-script/plugin execution instruction. |

## Verification

Run `cargo test --locked --offline --lib ifds_k017_` and the
[shared completion gate](README.md#unit-test-convention-and-completion-gate).
Also run offline contract integration tests against the actual pinned provider on
tiny tsconfig/import projects; mock responses alone cannot validate compiler resolution.
Run `cargo test --locked --offline --test ifds_metadata` for the real pinned
Bluesky web metadata and CLI gates. Its partial source slice is explicitly scoped
and does not replace K025 acceptance.

Completed verification: all seven required test names are registered; all 18
K017 unit tests and three metadata/CLI integration tests pass without ignored
cases. The full offline suite passes (317 tests, including doctests), as do
formatting, all-target Clippy with warnings denied, bridge syntax checking and
the documented CLI request. Pinned source/config fixture hashes were verified
against the manifest. Application code and package scripts were not executed.
