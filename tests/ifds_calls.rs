//! End-to-end K018 gates against the actual pinned TypeScript compiler bridge.
use repo_memory::ifds::adapters::typescript::{index_bindings, index_functions};
use repo_memory::ifds::*;
use std::collections::{BTreeMap, BTreeSet};

fn fixture(
    main: &str,
    helper_before: &str,
    helper_after: &str,
) -> (
    FunctionResultQuery,
    VariableFlowQuery,
    InMemorySnapshotProvider,
    AnalysisEnvironment,
    ProjectPairOptions,
) {
    let handle = |side, label: &str| SnapshotHandle {
        repository_id: "k018-integration".into(),
        id: SnapshotId {
            side,
            revision: label.into(),
            content_id: format!("tree-{label}"),
        },
    };
    let before = handle(SnapshotSide::Before, "before");
    let after = handle(SnapshotSide::After, "after");
    let snapshots = InMemorySnapshotProvider::new([(&before, helper_before, "helper-before"), (&after, helper_after, "helper-after")].into_iter().map(|(handle, helper, blob)| InMemorySnapshot { handle: handle.clone(), files: BTreeMap::from([("main.ts".into(), ("main".into(), main.as_bytes().to_vec())), ("helper.ts".into(), (blob.into(), helper.as_bytes().to_vec())), ("tsconfig.json".into(), ("config".into(), br#"{"compilerOptions":{"module":"esnext","moduleResolution":"bundler","target":"es2022"},"include":["*.ts"]}"#.to_vec()))]) }));
    let diff = RepositoryDiff {
        before_content_id: before.id.content_id.clone(),
        after_content_id: after.id.content_id.clone(),
        changes: BTreeSet::from([FileChange {
            kind: FileChangeKind::Modified,
            before_path: Some("helper.ts".into()),
            after_path: Some("helper.ts".into()),
            before_content_id: Some("helper-before".into()),
            after_content_id: Some("helper-after".into()),
        }]),
    };
    let capabilities = CapabilitySet {
        stage: 4,
        capabilities: BTreeSet::new(),
        version: "k018".into(),
    };
    let environment = AnalysisEnvironment {
        capabilities: capabilities.clone(),
        summaries: BTreeSet::new(),
    };
    let limits = AnalysisLimits {
        time_ms: 30_000,
        memory_bytes: 256_000_000,
        processed_path_edges: 100_000,
        output_nodes: 10_000,
        witnesses_per_relation: 10,
    };
    let function = index_functions("main.ts", main)
        .unwrap()
        .into_iter()
        .find(|f| f.name.as_deref() == Some("main"))
        .unwrap();
    let index = index_bindings(before.id.clone(), "main.ts", main).unwrap();
    let binding = index
        .bindings
        .iter()
        .find(|b| b.name == "language")
        .unwrap();
    let variable_query = VariableFlowQuery {
        before: before.clone(),
        after: after.clone(),
        diff: diff.clone(),
        selected_binding: BindingSelector {
            snapshot: before.id.clone(),
            declaration: binding.declaration.clone(),
            expected_name: Some(binding.name.clone()),
            expected_enclosing_symbol: binding.enclosing_symbol.clone(),
        },
        counterpart: None,
        entry: EntryPoint::ContainingFunction,
        capabilities: capabilities.clone(),
        summaries: BTreeSet::new(),
        limits: limits.clone(),
    };
    let function_query = FunctionResultQuery {
        selected_function: FunctionSelector {
            snapshot: before.id.clone(),
            declaration: function.span,
            expected_name: function.name,
            expected_enclosing_symbol: function.owner,
        },
        before,
        after,
        diff,
        counterpart: None,
        before_entry: FunctionEntry::default(),
        after_entry: FunctionEntry::default(),
        capabilities,
        summaries: BTreeSet::new(),
        limits,
    };
    let options = ProjectOptions {
        tsconfig: Some("tsconfig.json".into()),
        platform: "web".into(),
        ..ProjectOptions::default()
    };
    (
        function_query,
        variable_query,
        snapshots,
        environment,
        ProjectPairOptions {
            before: options.clone(),
            after: options,
        },
    )
}

fn literal(dependency: &ResultDependency, value: &str) -> bool {
    dependency.literal
        == Some(FunctionKnownValue::String {
            value: value.into(),
        })
        || dependency.inputs.iter().any(|input| literal(input, value))
}

#[test]
fn ifds_k018_real_provider_both_apis_and_helper_change() {
    let main = "import { decorate } from './helper'; export function main() { let language = decorate('en'); const second = decorate('de'); return language + second; }";
    let (function_query, variable_query, snapshots, environment, options) = fixture(
        main,
        "export function decorate(value: string) { return value + '!'; }",
        "export function decorate(value: string) { return value + '?'; }",
    );
    let provider = NodeMetadataProvider::default();
    let variable = analyze_variable_flow_with_metadata(
        variable_query,
        &snapshots,
        (&environment, &environment),
        options.clone(),
        &provider,
    )
    .unwrap()
    .analysis;
    assert_eq!(variable.completeness, Completeness::CompleteForQuery);
    assert!(
        variable
            .before_graph
            .nodes
            .iter()
            .any(|n| n.span.path == "helper.ts" && n.operation.contains("'!'"))
    );
    assert!(
        variable
            .after_graph
            .nodes
            .iter()
            .any(|n| n.span.path == "helper.ts" && n.operation.contains("'?'"))
    );
    assert!(
        variable
            .witnesses
            .iter()
            .any(|w| !w.call_contexts.is_empty())
    );
    VariableFlowReport::from_json(&variable.to_canonical_json().unwrap()).unwrap();
    let full = analyze_function_result_with_metadata(
        function_query,
        &snapshots,
        (&environment, &environment),
        options,
        &provider,
    )
    .unwrap()
    .analysis;
    for (side, expected) in [(&full.analysis.before, "!"), (&full.analysis.after, "?")] {
        let side = side.as_ref().unwrap();
        assert_eq!(side.coverage, Coverage::Complete);
        let ResultValue::Expression { dependency, .. } = &side.observations[0].value else {
            panic!("expected expression")
        };
        assert!(literal(dependency, expected));
        assert!(!dependency.unresolved);
    }
    assert!(full.comparison.regions.iter().all(|region| matches!(
        region.assessment,
        ResultAssessment::Changed | ResultAssessment::Different
    )));
}

#[test]
fn ifds_k018_real_provider_local_arrow_and_unknown_dispatch() {
    let main = "export function main() { const identity = (value: string) => value; let language = identity('en'); return language; }";
    let (query, _, snapshots, environment, options) =
        fixture(main, "export const unused = 1;", "export const unused = 2;");
    let report = analyze_function_result_with_metadata(
        query,
        &snapshots,
        (&environment, &environment),
        options,
        &NodeMetadataProvider::default(),
    )
    .unwrap();
    let side = report.analysis.analysis.before.as_ref().unwrap();
    assert_eq!(side.coverage, Coverage::Complete, "{:?}", side.diagnostics);
    let ResultValue::Expression { dependency, .. } = &side.observations[0].value else {
        panic!("expected expression")
    };
    assert!(literal(dependency, "en"));
    let main = "import { decorate } from './helper'; export function main(fn: (value: string) => string) { let language = fn('en'); return language; }";
    let (query, _, snapshots, environment, options) = fixture(
        main,
        "export function decorate(value: string) { return value; }",
        "export function decorate(value: string) { return value + '!'; }",
    );
    let report = analyze_function_result_with_metadata(
        query,
        &snapshots,
        (&environment, &environment),
        options,
        &NodeMetadataProvider::default(),
    )
    .unwrap();
    assert_eq!(
        report.analysis.analysis.before.as_ref().unwrap().coverage,
        Coverage::Partial
    );
    assert!(
        report
            .analysis
            .comparison
            .regions
            .iter()
            .all(|r| r.assessment == ResultAssessment::Unknown)
    );
}
