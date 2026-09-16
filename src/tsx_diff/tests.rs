use super::*;

fn analyze(before: &str, after: &str) -> Vec<SymbolChange> {
    let patch = diffy::create_patch(before, after).to_string();
    analyze_tsx_diff(&patch, before, after).unwrap()
}

fn summary(changes: &[SymbolChange]) -> Vec<(&str, ChangeKind, SymbolKind)> {
    changes
        .iter()
        .map(|change| {
            let symbol = change.after.as_ref().or(change.before.as_ref()).unwrap();
            (symbol.qualified_name.as_str(), change.kind, symbol.kind)
        })
        .collect()
}

#[test]
fn bluesky_pr_11683_detects_client_backoff_changes() {
    // Only src/analytics/metrics/client.ts from this PR; client.test.ts is excluded.
    // Full pinned snapshots and the original multi-hunk patch run offline.
    let original = include_str!("fixtures/bluesky_pr_11683/before.ts");
    let modified = include_str!("fixtures/bluesky_pr_11683/after.ts");
    let diff = include_str!("fixtures/bluesky_pr_11683/change.diff");
    let changes = analyze_tsx_diff(diff, original, modified)
        .expect("Bluesky PR #11683 client.ts should match its original Git diff");

    // Exact equality also excludes unchanged methods, fields, type declarations,
    // and local variables that merely moved as lines were inserted above them.
    assert_eq!(
        summary(&changes),
        vec![
            ("MAX_BACKOFF_MS", ChangeKind::Added, SymbolKind::Variable),
            ("MIN_BACKOFF_MS", ChangeKind::Added, SymbolKind::Variable),
            ("MetricsClient", ChangeKind::Changed, SymbolKind::Class),
            (
                "MetricsClient::backoffMs",
                ChangeKind::Added,
                SymbolKind::Property
            ),
            (
                "MetricsClient::backoffUntil",
                ChangeKind::Added,
                SymbolKind::Property
            ),
            (
                "MetricsClient::flush",
                ChangeKind::Changed,
                SymbolKind::Method
            ),
            (
                "MetricsClient::sendBatch",
                ChangeKind::Changed,
                SymbolKind::Method
            ),
            ("MetricsClient::trim", ChangeKind::Added, SymbolKind::Method),
        ]
    );

    let ranges: Vec<_> = changes
        .iter()
        .map(|change| {
            (
                change
                    .before
                    .as_ref()
                    .map(|symbol| (symbol.start_line, symbol.end_line)),
                change
                    .after
                    .as_ref()
                    .map(|symbol| (symbol.start_line, symbol.end_line)),
            )
        })
        .collect();
    assert_eq!(
        ranges,
        vec![
            (None, Some((24, 24))),
            (None, Some((23, 23))),
            (Some((17, 114)), Some((26, 150))),
            (None, Some((33, 33))),
            (None, Some((34, 34))),
            (Some((63, 67)), Some((74, 84))),
            (Some((69, 107)), Some((86, 133))),
            (None, Some((145, 149))),
        ]
    );
}

#[test]
fn bluesky_pr_11693_reports_only_the_changed_inner_component() {
    // Exact upstream snapshots and patch; see the fixture README for revisions
    // and attribution. The function is context; the precise edit is an added
    // style property on the outer View, not a new declaration.
    let original = include_str!("fixtures/bluesky_pr_11693/before.tsx");
    let modified = include_str!("fixtures/bluesky_pr_11693/after.tsx");
    let diff = include_str!("fixtures/bluesky_pr_11693/change.diff");

    let changes = analyze_tsx_diff(diff, original, modified)
        .expect("Bluesky PR #11693 should parse and match its original Git diff");

    let before = Symbol {
        name: "BottomSheetNativeComponentInner".into(),
        qualified_name: "BottomSheetNativeComponentInner".into(),
        kind: SymbolKind::Function,
        type_annotation: None,
        start_line: 125,
        end_line: 199,
    };
    let after = Symbol {
        end_line: 204,
        ..before.clone()
    };
    assert_eq!(
        changes,
        vec![SymbolChange {
            kind: ChangeKind::Changed,
            before: Some(before),
            after: Some(after),
            child_changes: vec![ChildChange {
                kind: ChangeKind::Added,
                target: "View".into(),
                target_path: "NativeView[0]/View[0]".into(),
                input: "style".into(),
                path: "style[2].flexShrink".into(),
                before: None,
                after: Some(ChangeValue {
                    text: "1".into(),
                    start_line: 189,
                    end_line: 189,
                }),
            }],
        }]
    );
}

#[test]
fn reports_added_removed_and_changed_jsx_props() {
    let changes = analyze(
        "const App = () => <Button title=\"Old\" disabled />;\n",
        "const App = () => <Button title=\"New\" onPress={handlePress} />;\n",
    );
    let details = &changes[0].child_changes;
    assert_eq!(details.len(), 3);
    assert_eq!(
        (details[0].path.as_str(), details[0].kind),
        ("disabled", ChangeKind::Removed)
    );
    assert_eq!(details[0].before.as_ref().unwrap().text, "true");
    assert!(details[0].after.is_none());
    assert_eq!(
        (details[1].path.as_str(), details[1].kind),
        ("onPress", ChangeKind::Added)
    );
    assert!(details[1].before.is_none());
    assert_eq!(details[1].after.as_ref().unwrap().text, "handlePress");
    assert_eq!(
        (details[2].path.as_str(), details[2].kind),
        ("title", ChangeKind::Changed)
    );
    assert_eq!(details[2].before.as_ref().unwrap().text, "\"Old\"");
    assert_eq!(details[2].after.as_ref().unwrap().text, "\"New\"");
    assert!(
        details
            .iter()
            .all(|d| d.target == "Button" && d.target_path == "Button[0]")
    );
}

#[test]
fn reports_nested_style_property_changes_without_unchanged_siblings() {
    let changes = analyze(
        "function App() { return <View style={{opacity: 1, shadow: {width: 2, height: 3}, color: 'red'}} /> }\n",
        "function App() { return <View style={{opacity: 0, shadow: {width: 4, height: 3}}} /> }\n",
    );
    let details = &changes[0].child_changes;
    let paths: Vec<_> = details.iter().map(|d| (d.path.as_str(), d.kind)).collect();
    assert_eq!(
        paths,
        vec![
            ("style.color", ChangeKind::Removed),
            ("style.opacity", ChangeKind::Changed),
            ("style.shadow.width", ChangeKind::Changed),
        ]
    );
    assert_eq!(details[2].before.as_ref().unwrap().text, "2");
    assert_eq!(details[2].after.as_ref().unwrap().text, "4");
}

#[test]
fn distinguishes_repeated_components_and_ignores_attribute_order() {
    let changes = analyze(
        "function App() { return <><View id=\"first\" style={{flex: 1}} /><View style={{flex: 2}} id=\"second\" /></> }\n",
        "function App() { return <><View style={{flex: 1}} id=\"first\" /><View id=\"second\" style={{flex: 3}} /></> }\n",
    );
    let details = &changes[0].child_changes;
    assert_eq!(details.len(), 1);
    assert_eq!(details[0].target_path, "<fragment>[0]/View[1]");
    assert_eq!(details[0].path, "style.flex");
    assert_eq!(details[0].before.as_ref().unwrap().text, "2");
    assert_eq!(details[0].after.as_ref().unwrap().text, "3");
}

#[test]
fn comments_and_formatting_are_not_reported_as_object_property_changes() {
    let changes = analyze(
        "const App = () => <View style={{flex: 1, color: 'red'}} />;\n",
        "const App = () => <View style={{ /* layout */ color: 'red', flex: 1 }} />;\n",
    );
    assert_eq!(changes.len(), 1);
    assert!(changes[0].child_changes.is_empty());
}

#[test]
fn falls_back_to_expression_changes_for_guards_spreads_and_array_resizing() {
    for (before, after, expected_path) in [
        ("[android && {flex: 1}]", "[ios && {flex: 1}]", "style[0]"),
        ("{...base, flex: 1}", "{...base, flex: 2}", "style"),
        ("[{flex: 1}]", "[base, {flex: 1}]", "style"),
        ("{[key]: 1}", "{[key]: 2}", "style"),
        ("[, {flex: 1}]", "[, {flex: 2}]", "style"),
    ] {
        let original = format!("const App = () => <View style={{{before}}} />;\n");
        let modified = format!("const App = () => <View style={{{after}}} />;\n");
        let changes = analyze(&original, &modified);
        let details = &changes[0].child_changes;
        assert_eq!(details.len(), 1, "{before} -> {after}");
        assert_eq!(details[0].path, expected_path);
        assert_eq!(details[0].kind, ChangeKind::Changed);
    }
}

#[test]
fn retains_symbol_changes_for_edits_unrelated_to_jsx_props() {
    let changes = analyze(
        "function App() { return <View>Old</View> }\n",
        "function App() { return <View>New</View> }\n",
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ChangeKind::Changed);
    assert!(changes[0].child_changes.is_empty());
}

#[test]
fn detects_added_changed_and_removed_symbols_in_git_diff() {
    let before = "const gone: number = 1;\nexport function View() { return <div>Old</div>; }\n";
    let after = "const added: boolean = true;\nexport function View() { return <div>New</div>; }\n";
    let diff = "diff --git a/view.tsx b/view.tsx\nindex 1234567..abcdef0 100644\n--- a/view.tsx\n+++ b/view.tsx\n@@ -1,2 +1,2 @@\n-const gone: number = 1;\n-export function View() { return <div>Old</div>; }\n+const added: boolean = true;\n+export function View() { return <div>New</div>; }\n";
    let changes = analyze_tsx_diff(diff, before, after).unwrap();
    assert_eq!(
        summary(&changes),
        vec![
            ("View", ChangeKind::Changed, SymbolKind::Function),
            ("added", ChangeKind::Added, SymbolKind::Variable),
            ("gone", ChangeKind::Removed, SymbolKind::Variable),
        ]
    );
    assert!(changes[1].before.is_none());
    assert!(changes[2].after.is_none());
    assert_eq!(
        changes[1]
            .after
            .as_ref()
            .unwrap()
            .type_annotation
            .as_deref(),
        Some("boolean")
    );
    assert_eq!(changes[0].before.as_ref().unwrap().start_line, 2);
}

#[test]
fn extracts_declaration_kinds_and_scoped_members() {
    let source = "\
interface Props { label: string; render(): void; }
type Label = string;
enum Mode { On, Off }
class View { title: string = 'Hi'; render(): string { return this.title; } }
const Button = (): JSX.Element => <button />;
namespace UI { export function show() {} }
";
    let changes = analyze("", source);
    assert_eq!(
        summary(&changes),
        vec![
            ("Button", ChangeKind::Added, SymbolKind::Function),
            ("Label", ChangeKind::Added, SymbolKind::TypeAlias),
            ("Mode", ChangeKind::Added, SymbolKind::Enum),
            ("Props", ChangeKind::Added, SymbolKind::Interface),
            ("Props::label", ChangeKind::Added, SymbolKind::Property),
            ("Props::render", ChangeKind::Added, SymbolKind::Method),
            ("UI", ChangeKind::Added, SymbolKind::Namespace),
            ("UI::show", ChangeKind::Added, SymbolKind::Function),
            ("View", ChangeKind::Added, SymbolKind::Class),
            ("View::render", ChangeKind::Added, SymbolKind::Method),
            ("View::title", ChangeKind::Added, SymbolKind::Property),
        ]
    );
    assert_eq!(
        changes[0]
            .after
            .as_ref()
            .unwrap()
            .type_annotation
            .as_deref(),
        Some("JSX.Element")
    );
}

#[test]
fn ignores_context_symbols_and_line_shifts() {
    let before = "function stable() { return <div />; }\nconst value = 1;\n";
    let after = "// new heading\n\nfunction stable() { return <div />; }\nconst value = 2;\n";
    let changes = analyze(before, after);
    assert_eq!(
        summary(&changes),
        vec![("value", ChangeKind::Changed, SymbolKind::Variable)]
    );
    assert_eq!(changes[0].before.as_ref().unwrap().start_line, 2);
    assert_eq!(changes[0].after.as_ref().unwrap().start_line, 4);
}

#[test]
fn treats_rename_as_removal_and_addition() {
    let changes = analyze("function oldName() {}\n", "function newName() {}\n");
    assert_eq!(
        summary(&changes),
        vec![
            ("newName", ChangeKind::Added, SymbolKind::Function),
            ("oldName", ChangeKind::Removed, SymbolKind::Function),
        ]
    );
}

#[test]
fn detects_kind_and_explicit_type_changes() {
    let changes = analyze(
        "const value: number = 1;\n",
        "const value = (): string => 'one';\n",
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ChangeKind::Changed);
    assert_eq!(
        changes[0].before.as_ref().unwrap().kind,
        SymbolKind::Variable
    );
    assert_eq!(
        changes[0].after.as_ref().unwrap().kind,
        SymbolKind::Function
    );
    assert_eq!(
        changes[0]
            .before
            .as_ref()
            .unwrap()
            .type_annotation
            .as_deref(),
        Some("number")
    );
    assert_eq!(
        changes[0]
            .after
            .as_ref()
            .unwrap()
            .type_annotation
            .as_deref(),
        Some("string")
    );
}

#[test]
fn distinguishes_members_in_different_classes() {
    let before = "class A { render() { return <a />; } }\nclass B { render() { return <b />; } }\n";
    let after = before.replace("<b />", "<span />");
    assert_eq!(
        summary(&analyze(before, &after)),
        vec![
            ("B", ChangeKind::Changed, SymbolKind::Class),
            ("B::render", ChangeKind::Changed, SymbolKind::Method),
        ]
    );
}

#[test]
fn handles_empty_diff_and_metadata_only_diff() {
    let source = "const x = 1;\n";
    assert!(analyze_tsx_diff("", source, source).unwrap().is_empty());
    assert!(
        analyze_tsx_diff(
            "diff --git a/a.tsx b/a.tsx\nold mode 100644\nnew mode 100755\n",
            source,
            source
        )
        .unwrap()
        .is_empty()
    );
    assert_eq!(
        analyze_tsx_diff("", source, ""),
        Err(DiffError::ContentMismatch)
    );
}

#[test]
fn handles_new_and_deleted_files_without_final_newlines() {
    let source = "const Greeting = () => <p>Привіт 🌍</p>;";
    let added = analyze("", source);
    let removed = analyze(source, "");
    assert_eq!(
        summary(&added),
        vec![("Greeting", ChangeKind::Added, SymbolKind::Function)]
    );
    assert_eq!(
        summary(&removed),
        vec![("Greeting", ChangeKind::Removed, SymbolKind::Function)]
    );
    assert_eq!(added[0].after.as_ref().unwrap().end_line, 1);
}

#[test]
fn handles_zero_context_multiple_hunks() {
    let before = "const first = 1;\nconst stable = 0;\nconst last = 1;\n";
    let after = "const first = 2;\nconst stable = 0;\nconst last = 2;\n";
    let diff = "--- a/a.tsx\n+++ b/a.tsx\n@@ -1 +1 @@\n-const first = 1;\n+const first = 2;\n@@ -3 +3 @@\n-const last = 1;\n+const last = 2;\n";
    assert_eq!(
        summary(&analyze_tsx_diff(diff, before, after).unwrap()),
        vec![
            ("first", ChangeKind::Changed, SymbolKind::Variable),
            ("last", ChangeKind::Changed, SymbolKind::Variable),
        ]
    );
}

#[test]
fn rejects_malformed_mismatched_and_multi_file_diffs() {
    assert!(matches!(
        analyze_tsx_diff("diff --git a/a b/a\n@@broken\n", "", ""),
        Err(DiffError::InvalidDiff(_))
    ));
    assert!(matches!(
        analyze_tsx_diff("not a diff", "", ""),
        Err(DiffError::InvalidDiff(_))
    ));
    assert!(matches!(
        analyze_tsx_diff("@@ broken @@\n", "", ""),
        Err(DiffError::InvalidDiff(_))
    ));
    let patch = diffy::create_patch("const a = 1;\n", "const a = 2;\n").to_string();
    assert_eq!(
        analyze_tsx_diff(&patch, "const a = 3;\n", "const a = 2;\n"),
        Err(DiffError::ContentMismatch)
    );
    assert_eq!(
        analyze_tsx_diff(&patch, "const a = 1;\n", "const a = 3;\n"),
        Err(DiffError::ContentMismatch)
    );
    assert!(matches!(
        analyze_tsx_diff(&format!("{patch}{patch}"), "", ""),
        Err(DiffError::InvalidDiff(_))
    ));
    assert!(matches!(
        analyze_tsx_diff("diff --git a/a b/a\ndiff --git a/b b/b\n", "", ""),
        Err(DiffError::InvalidDiff(_))
    ));
    assert!(matches!(
        analyze_tsx_diff("GIT binary patch\n", "", ""),
        Err(DiffError::InvalidDiff(_))
    ));
}

#[test]
fn rejects_invalid_tsx_in_either_snapshot() {
    let invalid = "const = <div>;\n";
    let patch = diffy::create_patch("", invalid).to_string();
    assert_eq!(
        analyze_tsx_diff(&patch, "", invalid),
        Err(DiffError::InvalidSyntax {
            snapshot: "modified"
        })
    );
    let patch = diffy::create_patch(invalid, "").to_string();
    assert_eq!(
        analyze_tsx_diff(&patch, invalid, ""),
        Err(DiffError::InvalidSyntax {
            snapshot: "original"
        })
    );
}

#[test]
fn compares_each_variable_and_captures_declaration_modifiers() {
    assert_eq!(
        summary(&analyze("const a = 1, b = 2;\n", "const a = 1, b = 3;\n")),
        vec![("b", ChangeKind::Changed, SymbolKind::Variable),]
    );
    for (before, after) in [
        ("const a = 1;\n", "let a = 1;\n"),
        ("const a = 1;\n", "export const a = 1;\n"),
        ("function a() {}\n", "export function a() {}\n"),
    ] {
        let changes = analyze(before, after);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, ChangeKind::Changed);
    }
}

#[test]
fn handles_destructuring_without_reporting_property_keys_or_default_references() {
    let changes = analyze(
        "",
        "const { key: renamed, fallback = initial, nested: { item }, ...rest } = data;\nconst [first, , third = initial] = items;\n",
    );
    let names: Vec<_> = changes
        .iter()
        .map(|c| c.after.as_ref().unwrap().name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["fallback", "first", "item", "renamed", "rest", "third"]
    );
}

#[test]
fn handles_anonymous_default_exports() {
    for (source, kind) in [
        (
            "export default function() { return <div />; }\n",
            SymbolKind::Function,
        ),
        ("export default class {}\n", SymbolKind::Class),
    ] {
        assert_eq!(
            summary(&analyze("", source)),
            vec![("default", ChangeKind::Added, kind)]
        );
    }
}

#[test]
fn preserves_overloads_with_duplicate_names() {
    let before = "function f(x: string): string;\nfunction f(x: any): any { return x; }\n";
    let after = before.replace("x: string", "x: number");
    let changes = analyze(before, &after);
    assert_eq!(
        summary(&changes),
        vec![("f", ChangeKind::Changed, SymbolKind::Function)]
    );
    assert_eq!(changes[0].before.as_ref().unwrap().start_line, 1);
}

#[test]
fn moving_unchanged_declarations_does_not_report_changes() {
    assert!(
        analyze(
            "const a = 1;\nconst b = 2;\n",
            "const b = 2;\nconst a = 1;\n"
        )
        .is_empty()
    );
}
