# K017 — TypeScript project metadata and modules

Status: planned. Roadmap stage: 4.

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
