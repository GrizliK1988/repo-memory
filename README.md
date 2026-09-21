# repo-memory

Rust utilities for analyzing repository changes.

The IFDS variable-flow analyzer has a separate
[specification and staged roadmap](docs/ifds/README.md), including automated
benchmark criteria and the Bluesky PR #5816 reference case. Stage 1 supports
straight-line scalar variable flow; later language features remain planned.
The [kickoff task set](spec/kickoff/README.md) breaks that design into ordered,
unit-test-verifiable implementation tasks.

To inspect a real stage-1 report from two local TypeScript files, run:

```sh
cargo run --locked --offline --example ifds_compare -- \
  tests/ifds/fixtures/overwrite/before/main.ts \
  tests/ifds/fixtures/overwrite/after/main.ts x > report.json
```

The example prints the full JSON report, including `human_summary`, flow graphs,
precise deltas, witnesses, coverage, and unknown boundaries. Replace the two paths
with your own `.ts` or `.tsx` files and `x` with a declaration name that occurs once
in each file. The example analyzes the selected binding's containing procedure.

`tsx_diff::analyze_tsx_diff(git_diff, original, modified)` returns added, removed,
and changed declarations using the [Tree-sitter TSX grammar](https://docs.rs/tree-sitter-typescript/0.23.2/tree_sitter_typescript/constant.LANGUAGE_TSX.html).
Pass a single file's complete unified diff and both UTF-8 snapshots. For new or
deleted files, pass an empty string for the missing snapshot.

```rust
use repo_memory::tsx_diff::{analyze_tsx_diff, ChangeKind, SymbolKind};

let original = "const Button = () => <button>Old</button>;\n";
let modified = "const Button = () => <button>New</button>;\n";
let diff = "--- a/Button.tsx\n+++ b/Button.tsx\n@@ -1 +1 @@\n-const Button = () => <button>Old</button>;\n+const Button = () => <button>New</button>;\n";
let changes = analyze_tsx_diff(diff, original, modified)?;
assert_eq!(changes[0].kind, ChangeKind::Changed);
assert_eq!(changes[0].after.as_ref().unwrap().kind, SymbolKind::Function);
# Ok::<(), repo_memory::tsx_diff::DiffError>(())
```

Each change includes before/after symbol metadata: name, qualified name, declaration
kind, explicit type annotation when available, and one-based inclusive line range.
Kinds include functions (including arrow-function variables), variables, classes,
methods, properties, interfaces, type aliases, enums, and namespaces. Variable
destructuring is supported; anonymous default-export functions/classes use `default`.

Each result also includes `child_changes: Vec<ChildChange>` for changes within
child constructs. The language-independent types live in `repo_memory::changes`
and are also re-exported from `tsx_diff`. Each detail identifies a `target`, its
`target_path`, the affected `input`, a nested `path`, and before/after `ChangeValue`
expression text and lines. These names can also describe a called function and its
arguments when another adapter supports them.

The current TSX adapter reports JSX props and nested inline object properties;
function-call argument detection and other languages are not implemented yet.
For JSX, `target` is the component, `target_path` is its JSX ancestry plus zero-based
occurrences per tag, and `input` is the prop name. Implicit boolean props have the
normalized value `true`.
For the Bluesky PR #11693 fixture, the function remains the enclosing context and
the detail is:

```text
Function:  BottomSheetNativeComponentInner
Component: View (NativeView[0]/View[0])
Change:    Added style[2].flexShrink = 1, modified line 189
```

```rust
use repo_memory::changes::ChangeKind;
use repo_memory::tsx_diff::analyze_tsx_diff;

let original = "const App = () => <View style={{flex: 1}} />;\n";
let modified = "const App = () => <View style={{flex: 1, flexShrink: 1}} />;\n";
let diff = diffy::create_patch(original, modified).to_string();
let changes = analyze_tsx_diff(&diff, original, modified)?;
let detail = &changes[0].child_changes[0];
assert_eq!(detail.kind, ChangeKind::Added);
assert_eq!(detail.target, "View");
assert_eq!(detail.input, "style");
assert_eq!(detail.path, "style.flexShrink");
assert_eq!(detail.after.as_ref().unwrap().text, "1");
# Ok::<(), repo_memory::tsx_diff::DiffError>(())
```

The diff is parsed and applied in memory to verify that it produces the supplied
modified contents. Both snapshots are then parsed and their declarations compared.
Malformed patches, binary/combined or multi-file diffs, mismatched contents, and TSX
syntax errors return errors. Metadata-only diffs are accepted for identical contents.

Symbols are matched by declaration ancestry and name; repeated names (such as
overloads) are matched by occurrence in source order. Renames appear as removal plus
addition. Unchanged declarations that move within a scope are not reported. Text
edits inside declarations, including formatting/comments, count as changes, and
enclosing declarations are reported alongside changed members. Export and variable
declaration modifiers also count. Comments outside declarations do not.

This module does not infer TypeScript types or resolve references. Imports,
parameters, enum members, and standalone export lists are not individual symbols.
Anonymous blocks do not establish separate matching scopes. Occurrence matching
can be ambiguous when duplicate declarations are inserted, removed, or reordered.
React wrappers such as `memo(...)` remain variable declarations; no React-specific
component classification is inferred.

JSX elements are matched by tag, JSX ancestry, and occurrence within the declaration.
Inserting or reordering repeated tags can make this matching ambiguous; React `key`
values are not used for identity. Added/removed elements report their explicit props
as added/removed. Details are also included in enclosing declarations, so a class
and its method can both contain the same prop edit.

Property comparisons descend into inline objects, arrays of unchanged length, and
the right side of `&&` expressions when the condition is unchanged. Array resizing,
object spreads, computed/duplicate keys, and other expressions fall back to reporting
the enclosing expression as changed. Object formatting, comments, and property order
alone do not produce property details. Values are source expressions, not evaluated
runtime values. Referenced style variables and JSX spreads (`{...props}`) are not
resolved. JSX text/children, spread-only changes, and other function edits still
produce declaration changes but may have no prop details; an empty details list
does not imply unchanged behavior.

Run `cargo test` for unit tests and `cargo doc --no-deps` for API documentation.
