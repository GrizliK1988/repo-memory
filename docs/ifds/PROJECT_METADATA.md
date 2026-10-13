# K017: snapshot-backed TypeScript project metadata

K017 resolves declarations, imports/re-exports and direct call references and
attaches that evidence to scalar IR. [K018](INTERPROCEDURAL_CALLS.md) uses supported
resolved bodies for argument/return transfers and matched summaries in the two
metadata-backed analysis APIs. Unknown targets and unmodeled effects stay open. Module initialization belongs to K037. No application source is executed.

## Environment and setup

Node.js 18+ is an explicit prerequisite. The tool's bridge uses TypeScript 5.9.3
from its own package directory, with an exact version and npm integrity lock. It
does not use the analyzed project's installed compiler. Provision dependencies
before analysis, for example:

```sh
npm ci --prefix tools/typescript-metadata --ignore-scripts --no-audit --no-fund
```

The analyzer never installs Node/packages, accesses the network, invokes project
scripts, loads custom compiler plugins, or emits/executes project JavaScript.
The Rust provider starts one bounded Node process per snapshot, with concurrent
pipe handling, a default 30-second timeout, a 128-MiB input/output limit, and a
512-MiB Node heap limit. These are provider limits, independent of solver budgets.

## Request and explicit configuration

`MetadataRequest` protocol 1 contains a snapshot identity, repository-relative
files with content identities and text, selected paths and `ProjectOptions`.
`MetadataRequest::from_snapshot` reads an existing immutable `SnapshotProvider`;
it does not inspect the working tree. JSON CLI callers supply the same immutable
file map directly and are responsible for truthful snapshot/file identities.

```json
{
  "protocol_version": 1,
  "compiler_version": "5.9.3",
  "snapshot": {
    "side": "before",
    "revision": "example-revision",
    "content_id": "example-tree"
  },
  "files": {
    "tsconfig.json": {
      "content_id": "config-v1",
      "text": "{\"compilerOptions\":{\"module\":\"esnext\",\"moduleResolution\":\"bundler\",\"paths\":{\"#/*\":[\"./src/*\"]}},\"include\":[\"src\"]}"
    },
    "src/main.ts": {
      "content_id": "main-v1",
      "text": "import { helper } from '#/helper'; export function main(value: string) { return helper(value); }"
    },
    "src/helper.ts": {
      "content_id": "helper-v1",
      "text": "export function helper(value: string) { return value; }"
    }
  },
  "selected_paths": ["src/main.ts"],
  "options": {
    "tsconfig": "tsconfig.json",
    "platform": "web",
    "runtime_aliases": { "#/*": "src/*" },
    "excluded_paths": [],
    "supplied_files": {}
  }
}
```

An explicit config wins. With `tsconfig: null`, discovery succeeds only when one
config covers all selected paths. Ambiguity returns `ambiguous_tsconfig` with
candidates. A missing config returns `missing_tsconfig`; a missing `extends`
target returns `missing_base_config` with snapshot, requester and specifier.
Invalid/cyclic configs are input errors. There is no silent default-config mode.

An agent can supply configs as data under their virtual resolution paths, e.g.
`node_modules/@react-native/typescript-config/tsconfig.json`. The supplied map
belongs to one snapshot; before/after can require different contents. Explicit
`extends` chains and package config manifests are processed by the compiler,
inside the virtual file system. Supplied files cannot replace snapshot files or
provide executable dependency bodies. JSON configs and declaration files are
accepted; generated application source absent from the snapshot stays missing.

The selected config controls paths, extensions, module resolution, conditional
exports, libs and module suffixes. `platform` records the declared environment;
it is not an automatic configuration rewriter. Use the project's web config for
web. Optional static `runtime_aliases` assert mappings from build configuration;
disagreement with compiler resolution is `runtime_mismatch`. Executable bundler
configuration is not evaluated. General workspace/package-link modeling is not
part of the first Bluesky example. A Node runtime combined with bundler-only
resolution is conservatively marked incompatible for resolved imported bodies.

## Results and uncertainty

Metadata records the pinned compiler/protocol, exact configuration inputs,
effective compiler options, runtime aliases, selected/excluded paths, declarations,
imports, call signatures, candidates and source-positioned resolution chains.
Supported direct references include ordinary function declarations, const function
initializers, default exports and namespace-import function members. Reassigned
function bindings, type-only imports, method dispatch and function-valued aliases
cannot acquire a unique invocation target from types alone. Unsupported syntax
and unmapped call operations remain explicit.

External packages are labeled `external_package`, including the package name;
available declaration signatures provide no purity or return-origin guarantee.
Missing internal modules are `missing_dependency`; excluded bodies are
`excluded_body`; declaration-file signatures are `declaration_only`. Bodyless
local declarations also retain unknown behavior. Unresolved/ambiguous/runtime-incompatible calls keep no unique
target. Type errors elsewhere do not block resolution; this is not a type-check
command. External bodies are not analyzed even when snapshot paths include them.

`SnapshotMetadataReport` maps call indices to operation IDs and source declarations
to procedure IDs. Paths are part of every mapping: existing scalar IR IDs are
file-local. UTF-16 compiler offsets are converted to UTF-8 byte spans once per
source file; Rust checks spans against the supplied text. Export wrappers,
function initializers, nested calls and overload signatures are handled separately.
Top-level mapping uses only the existing single-file program IR and does not infer
imported module execution. Calls inside unsupported async/class syntax can have
resolution metadata without an IR mapping. `unmapped_calls` and diagnostics record
calls with no emitted IR operation, without implying that they execute.

`analyze_variable_flow_with_metadata` and `analyze_function_result_with_metadata`
return additive `ProjectAnalysisReport<T>` envelopes (schema 1). Existing scalar
analysis/report schemas and semantics remain intact. Metadata unavailability
preserves local analysis with a `TypeResolutionUnavailable` diagnostic. Invalid
configurations or mismatched/invalid provider responses fail explicitly.

## CLI

Save a request like the above as `request.json`, then run:

```sh
cargo run --locked --offline -- metadata --request request.json
cargo run --locked --offline -- metadata --request request.json --format text
cargo run --locked --offline -- metadata --request request.json --format text --verbose
```

JSON includes full chains and mappings. Text shows the call, target, status and
reason; verbose text adds the chain. `--tsconfig` selects a virtual config path.
Missing root/base configs can be supplied explicitly through repeatable flags:

```sh
cargo run --locked --offline -- metadata --request request.json \
  --tsconfig tsconfig.check.web.json \
  --config-file node_modules/@react-native/typescript-config/tsconfig.json=/tmp/base.json
```

This adds config data to that request, never to the snapshot or working tree.
CLI-supplied config identities are labeled FNV-1a content fingerprints; the full
text is retained and validated, so fingerprints are not used as a security proof.
`--node` overrides the Node executable; `--bridge` locates a separately distributed
bridge and its provisioned package directory. Config/provider failures are JSON
errors on stderr with a nonzero exit code. The standalone metadata command cannot
produce useful metadata when its provider is unavailable; the analysis APIs can
still preserve their local results.

## Verification

Provision the pinned compiler before offline verification. Tests use immutable
in-memory sources, independently authored mock contracts, and the actual bridge.
The pinned [Bluesky web slice](../../tests/ifds/metadata/bluesky-web/README.md)
checks real aliases, the exact inherited base config, known helper declarations,
distinct IR operations and explicit external/missing boundaries. This is a
partial-source metadata gate, not K025 flow acceptance or a benchmark result.

```sh
cargo test --locked --offline --lib ifds_k017_ -- --list
cargo test --locked --offline --lib ifds_k017_
cargo test --locked --offline --test ifds_metadata
cargo fmt --check
cargo test --locked --offline
cargo clippy --locked --offline --all-targets -- -D warnings
```
