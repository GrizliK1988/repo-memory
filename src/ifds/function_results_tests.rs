use super::*;
use crate::ifds::adapters::typescript::index_functions;
use crate::ifds::snapshots::{InMemorySnapshot, InMemorySnapshotProvider};

fn fixture(
    before: &str,
    after: &str,
) -> (
    FunctionResultQuery,
    InMemorySnapshotProvider,
    AnalysisEnvironment,
) {
    let path = "fixture.ts";
    let before_id = SnapshotId {
        side: SnapshotSide::Before,
        revision: "before".into(),
        content_id: "before-tree".into(),
    };
    let after_id = SnapshotId {
        side: SnapshotSide::After,
        revision: "after".into(),
        content_id: "after-tree".into(),
    };
    let before_handle = SnapshotHandle {
        id: before_id.clone(),
        repository_id: "fixture".into(),
    };
    let after_handle = SnapshotHandle {
        id: after_id.clone(),
        repository_id: "fixture".into(),
    };
    let function = index_functions(path, before).unwrap().remove(0);
    let provider = InMemorySnapshotProvider::new([
        InMemorySnapshot {
            handle: before_handle.clone(),
            files: BTreeMap::from([(
                path.into(),
                ("before-blob".into(), before.as_bytes().to_vec()),
            )]),
        },
        InMemorySnapshot {
            handle: after_handle.clone(),
            files: BTreeMap::from([(
                path.into(),
                ("after-blob".into(), after.as_bytes().to_vec()),
            )]),
        },
    ]);
    let capabilities = CapabilitySet {
        stage: 1,
        capabilities: BTreeSet::new(),
        version: "test".into(),
    };
    let query = FunctionResultQuery {
        before: before_handle,
        after: after_handle,
        diff: RepositoryDiff {
            before_content_id: "before-tree".into(),
            after_content_id: "after-tree".into(),
            changes: BTreeSet::from([FileChange {
                kind: FileChangeKind::Modified,
                before_path: Some(path.into()),
                after_path: Some(path.into()),
                before_content_id: Some("before-blob".into()),
                after_content_id: Some("after-blob".into()),
            }]),
        },
        selected_function: FunctionSelector {
            snapshot: before_id,
            declaration: function.span,
            expected_name: function.name,
            expected_enclosing_symbol: function.owner,
        },
        counterpart: None,
        before_entry: FunctionEntry::default(),
        after_entry: FunctionEntry::default(),
        capabilities: capabilities.clone(),
        summaries: BTreeSet::new(),
        limits: AnalysisLimits {
            time_ms: 30_000,
            memory_bytes: 256_000_000,
            processed_path_edges: 100_000,
            witnesses_per_relation: 10,
            output_nodes: 10_000,
        },
    };
    let environment = AnalysisEnvironment {
        capabilities,
        summaries: BTreeSet::new(),
    };
    (query, provider, environment)
}

fn analyze(before: &str, after: &str) -> FunctionResultAnalysis {
    let (query, provider, environment) = fixture(before, after);
    analyze_function_result(query, &provider, &environment, &environment).unwrap()
}

fn returned_texts(side: &FunctionSnapshotResult) -> BTreeSet<String> {
    side.observations
        .iter()
        .map(|observation| match &observation.value {
            ResultValue::Undefined => "undefined".into(),
            ResultValue::Expression { text, .. } => text.clone(),
        })
        .collect()
}

fn dependency_literals(dependency: &ResultDependency, out: &mut BTreeSet<String>) {
    if let Some(literal) = &dependency.literal {
        out.insert(format!("{literal:?}"));
    }
    for input in &dependency.inputs {
        dependency_literals(input, out);
    }
}

#[test]
fn ifds_fr001_function_selector() {
    let before = "function first() { return 1; } function second() { return 2; }";
    let after = "function first() { return 3; } function second() { return 2; }";
    let (query, provider, environment) = fixture(before, after);
    let result =
        analyze_function_result(query.clone(), &provider, &environment, &environment).unwrap();
    assert_eq!(
        result
            .before
            .as_ref()
            .unwrap()
            .function
            .expected_name
            .as_deref(),
        Some("first")
    );
    assert_eq!(
        returned_texts(result.before.as_ref().unwrap()),
        BTreeSet::from(["1".into()])
    );
    let mut stale = query;
    stale.selected_function.expected_name = Some("second".into());
    assert!(analyze_function_result(stale, &provider, &environment, &environment).is_err());
    let renamed = analyze(
        "function oldName() { return 1; }",
        "function newName() { return 1; }",
    );
    assert_eq!(renamed.counterpart_status, FunctionCounterpart::Unresolved);
    let (mut query, provider, environment) = fixture(
        "function oldName() { return 1; }",
        "function newName() { return 1; }",
    );
    let other = index_functions("fixture.ts", "function newName() { return 1; }")
        .unwrap()
        .remove(0);
    query.counterpart = Some(FunctionSelector {
        snapshot: query.after.id.clone(),
        declaration: other.span,
        expected_name: other.name,
        expected_enclosing_symbol: other.owner,
    });
    let explicit = analyze_function_result(query, &provider, &environment, &environment).unwrap();
    assert_eq!(explicit.counterpart_status, FunctionCounterpart::Matched);
    let (mut query, provider, environment) = fixture(
        "function café() { return 1; }",
        "function café() { return 1; }",
    );
    query.selected_function.declaration.byte_end =
        "function café() { return 1; }".find('é').unwrap() as u64 + 1;
    assert!(analyze_function_result(query, &provider, &environment, &environment).is_err());
}

#[test]
fn ifds_fr001_input_correspondence() {
    let result = analyze(
        "function value(first: string) { return first; }",
        "function value(renamed: string) { return renamed; }",
    );
    assert_eq!(result.input_mapping_coverage, Coverage::Complete);
    assert_eq!(result.input_correspondence.len(), 1);
    assert_eq!(result.input_correspondence[0].position, 0);
    assert_eq!(result.before.as_ref().unwrap().parameter_names, ["first"]);
    assert_eq!(result.after.as_ref().unwrap().parameter_names, ["renamed"]);
    let reordered = analyze(
        "function value(first: number, second: number) { return first; }",
        "function value(second: number, first: number) { return first; }",
    );
    assert_eq!(
        reordered.after.as_ref().unwrap().parameter_names,
        ["second", "first"]
    );
    assert_eq!(reordered.input_correspondence.len(), 2);
    let changed_arity = analyze(
        "function value(p: number) { return p; }",
        "function value(p: number, q: number) { return q; }",
    );
    assert_eq!(changed_arity.input_mapping_coverage, Coverage::Partial);
}

#[test]
fn ifds_fr001_entry_assumption_expressions() {
    let (mut query, provider, environment) = fixture(
        "function answer(flag: boolean, ready: boolean) { return 1; }",
        "function answer(flag: boolean, ready: boolean) { return 1; }",
    );
    query
        .before_entry
        .domains
        .extend([(0, PrimitiveDomain::Boolean), (1, PrimitiveDomain::Boolean)]);
    query.after_entry.domains = query.before_entry.domains.clone();
    query.before_entry.assumptions.push(AssumptionExpr::Binary {
        operator: AssumptionBinary::And,
        left: Box::new(AssumptionExpr::Input { index: 0 }),
        right: Box::new(AssumptionExpr::Input { index: 1 }),
    });
    query
        .after_entry
        .assumptions
        .push(AssumptionExpr::Input { index: 0 });
    let result = analyze_function_result(query, &provider, &environment, &environment).unwrap();
    let round_trip: FunctionResultAnalysis =
        serde_json::from_slice(&serde_json::to_vec(&result).unwrap()).unwrap();
    assert_eq!(round_trip, result);
    assert_ne!(
        result.query.before_entry.assumptions,
        result.query.after_entry.assumptions
    );
}

#[test]
fn ifds_fr001_literal_without_binding() {
    let result = analyze(
        "function value() { return 1; }",
        "function value() { return 2; }",
    );
    let before = result.before.as_ref().unwrap();
    assert_eq!(before.coverage, Coverage::Complete);
    assert_eq!(returned_texts(before), BTreeSet::from(["1".into()]));
    assert!(before.parameters.is_empty());
}

#[test]
fn ifds_fr001_all_normal_exits() {
    let result = analyze(
        "function value(a: boolean, b: boolean) { if (a) return 1; if (b) return; }",
        "function value(a: boolean, b: boolean) { if (a) return 1; if (b) return; return 2; }",
    );
    let before = result.before.as_ref().unwrap();
    assert_eq!(
        returned_texts(before),
        BTreeSet::from(["1".into(), "undefined".into()])
    );
    assert!(
        before
            .observations
            .iter()
            .any(|item| matches!(item.kind, ResultExitKind::Bare))
    );
    assert!(
        before
            .observations
            .iter()
            .any(|item| matches!(item.kind, ResultExitKind::Fallthrough))
    );
    let arrow = analyze(
        "const value = (flag: boolean) => flag ? 1 : 2;",
        "const value = (flag: boolean) => flag ? 1 : 3;",
    );
    assert!(
        arrow
            .before
            .as_ref()
            .unwrap()
            .observations
            .iter()
            .any(|item| matches!(item.kind, ResultExitKind::Arrow))
    );
}

#[test]
fn ifds_fr001_nested_and_unreachable() {
    let result = analyze(
        "function outer() { function inner() { return 9; } return 1; return 8; }",
        "function outer() { function inner() { return 7; } return 1; return 8; }",
    );
    assert_eq!(
        returned_texts(result.before.as_ref().unwrap()),
        BTreeSet::from(["1".into()])
    );
}

#[test]
fn ifds_fr001_whole_expression() {
    let result = analyze(
        "function value(x: number) { return x + 4; }",
        "function value(x: number) { return x + 5; }",
    );
    let item = &result.before.as_ref().unwrap().observations[0];
    let ResultValue::Expression {
        text, dependency, ..
    } = &item.value
    else {
        panic!("expected expression")
    };
    assert_eq!(text, "x + 4");
    assert_eq!(dependency.inputs.len(), 2);
    let mut literals = BTreeSet::new();
    dependency_literals(dependency, &mut literals);
    assert!(literals.iter().any(|literal| literal.contains('4')));
}

#[test]
fn ifds_fr001_overwrites_and_copies() {
    let overwritten = analyze(
        "function value(p: number) { let x = p; x = 3; return x; }",
        "function value(q: number) { let x = q; x = 3; return x; }",
    );
    let item = &overwritten.before.as_ref().unwrap().observations[0];
    let ResultValue::Expression { dependency, .. } = &item.value else {
        panic!("expected expression")
    };
    let mut literals = BTreeSet::new();
    dependency_literals(dependency, &mut literals);
    assert!(literals.iter().any(|literal| literal.contains('3')));
    let copy = analyze(
        "function value() { let x = 1; const y = x; x = 3; return y; }",
        "function value() { let x = 2; const y = x; x = 3; return y; }",
    );
    let item = &copy.before.as_ref().unwrap().observations[0];
    let ResultValue::Expression { dependency, .. } = &item.value else {
        panic!("expected expression")
    };
    let mut literals = BTreeSet::new();
    dependency_literals(dependency, &mut literals);
    assert!(literals.iter().any(|literal| literal.contains('1')));
    assert!(!literals.iter().any(|literal| literal.contains('3')));
}

#[test]
fn ifds_fr001_effective_guards() {
    let result = analyze(
        "function value(a: boolean, b: boolean) { if (!a) return 0; if (b) return 1; return 2; }",
        "function value(a: boolean, b: boolean) { if (!a) return 0; if (b) return 3; return 2; }",
    );
    let side = result.before.as_ref().unwrap();
    assert!(side.observations.iter().any(|item| item.guards.len() >= 2));
    assert!(side.observations.iter().any(|item| {
        item.effective_condition
            .as_deref()
            .is_some_and(|condition| condition.contains('!'))
    }));
}

#[test]
fn ifds_fr001_unknown_completion() {
    let result = analyze(
        "function value(flag: boolean) { if (flag) return 1; mystery(); return 2; }",
        "function value(flag: boolean) { if (flag) return 3; mystery(); return 2; }",
    );
    let side = result.before.as_ref().unwrap();
    assert_eq!(side.coverage, Coverage::Partial);
    assert!(!side.unknown_boundaries.is_empty());
    assert!(
        side.observations
            .iter()
            .any(|item| item.unknown_completion_before_return)
    );
    assert!(
        side.observations
            .iter()
            .any(|item| !item.unknown_completion_before_return)
    );
}

#[test]
fn ifds_fr001_scope_and_limits() {
    let result = analyze(
        "function value() { return 1; }",
        "function value() { return 2; }",
    );
    assert!(result.before.as_ref().unwrap().observations[0].caller_continuation_open);
    let (mut query, provider, environment) = fixture(
        "function value(flag: boolean) { if (flag) return 1; return 2; }",
        "function value(flag: boolean) { if (flag) return 3; return 2; }",
    );
    query.limits.processed_path_edges = 1;
    let partial = analyze_function_result(query, &provider, &environment, &environment).unwrap();
    assert!(
        partial.before.as_ref().unwrap().coverage != Coverage::Complete
            || !partial.before.as_ref().unwrap().limits_hit.is_empty()
    );
}
