use repo_memory::ifds::adapters::typescript::index_functions;
use repo_memory::ifds::{
    AnalysisEnvironment, AnalysisLimits, CapabilitySet, CommonDomainStatus, Coverage, FileChange,
    FileChangeKind, FunctionEntry, FunctionKnownValue, FunctionResultQuery, FunctionSelector,
    InMemorySnapshot, InMemorySnapshotProvider, PrimitiveDomain, RepositoryDiff, ResultAssessment,
    SnapshotHandle, SnapshotId, SnapshotSide, analyze_function_result_reports,
};
use std::collections::{BTreeMap, BTreeSet};

fn reports(
    before: &str,
    after: &str,
    before_entry: FunctionEntry,
    after_entry: FunctionEntry,
) -> (
    repo_memory::ifds::FunctionResultReport,
    repo_memory::ifds::FunctionResultCompactReport,
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
        repository_id: "function-fixture".into(),
    };
    let after_handle = SnapshotHandle {
        id: after_id.clone(),
        repository_id: "function-fixture".into(),
    };
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
    let function = index_functions(path, before).unwrap().remove(0);
    let capabilities = CapabilitySet {
        stage: 1,
        capabilities: BTreeSet::new(),
        version: "integration-test".into(),
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
        before_entry,
        after_entry,
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
    analyze_function_result_reports(query, &provider, &environment, &environment).unwrap()
}

fn assert_region(
    report: &repo_memory::ifds::FunctionResultReport,
    condition: &[(u32, bool)],
    before: &str,
    after: &str,
    assessment: ResultAssessment,
) {
    let condition: BTreeMap<_, _> = condition.iter().copied().collect();
    assert!(
        report.comparison.regions.iter().any(|region| {
            region.region.values == condition
                && region.before_result.as_deref() == Some(before)
                && region.after_result.as_deref() == Some(after)
                && region.assessment == assessment
        }),
        "missing region {condition:?}: {before} -> {after} ({assessment:?}); {:#?}",
        report.comparison.regions
    );
}

#[test]
fn short_circuit_identity_reports_and_skipped_calls() {
    for (operator, skipped) in [("&&", false), ("||", true)] {
        let before = format!("function result(flag) {{ return flag {operator} \"old\"; }}");
        for rhs in ["\"new\"", "mystery()"] {
            let after = format!("function result(flag) {{ return flag {operator} {rhs}; }}");
            let (full, compact) = reports(
                &before,
                &after,
                FunctionEntry::default(),
                FunctionEntry::default(),
            );
            let text = compact.render_text(&full);
            assert_region(
                &full,
                &[(0, skipped)],
                &format!("flag {operator} \"old\""),
                &format!("flag {operator} {rhs}"),
                ResultAssessment::Equal,
            );
            assert_eq!(compact.equal_regions.len(), 1);
            assert!(!text.contains("Equal"), "{text}");
            assert!(!text.contains("Unchanged choices"), "{text}");
            if rhs == "mystery()" {
                assert_eq!(compact.coverage.result_comparison, Coverage::Partial);
                assert_eq!(compact.unknown_regions.len(), 1);
                assert_eq!(
                    compact.unknown_regions[0].region.values,
                    BTreeMap::from([(0, !skipped)])
                );
                assert!(text.contains("Unknown under"), "{text}");
                assert!(text.contains("unresolved call"), "{text}");
            } else {
                assert_eq!(compact.coverage.result_comparison, Coverage::Complete);
                assert!(compact.unknown_regions.is_empty());
                let condition = if skipped { "falsy" } else { "truthy" };
                assert!(
                    text.contains(&format!(
                        "when flag is {condition}: \"old\" -> \"new\" (Different)"
                    )),
                    "{text}"
                );
                assert!(!text.contains("Return choice"), "{text}");
            }
            compact.validate_with_full(&full).unwrap();
            let decoded_full = repo_memory::ifds::FunctionResultReport::from_json(
                &serde_json::to_vec(&full).unwrap(),
            )
            .unwrap();
            let decoded_compact = repo_memory::ifds::FunctionResultCompactReport::from_json(
                &serde_json::to_vec(&compact).unwrap(),
            )
            .unwrap();
            assert_eq!(decoded_full, full);
            assert_eq!(decoded_compact, compact);
            decoded_compact.validate_with_full(&decoded_full).unwrap();
        }
    }
    // Unlike &&, ?? skips its RHS for non-nullish falsy inputs as well.
    for value in [
        FunctionKnownValue::Boolean { value: false },
        FunctionKnownValue::Number { value: "0".into() },
        FunctionKnownValue::String {
            value: String::new(),
        },
        FunctionKnownValue::Null,
        FunctionKnownValue::Undefined,
    ] {
        let calls = matches!(
            value,
            FunctionKnownValue::Null | FunctionKnownValue::Undefined
        );
        let mut scope = FunctionEntry::default();
        scope.known_values.insert(0, value);
        let (full, compact) = reports(
            "function result(flag) { return flag ?? \"old\"; }",
            "function result(flag) { return flag ?? mystery(); }",
            scope.clone(),
            scope,
        );
        if calls {
            assert_eq!(compact.coverage.result_comparison, Coverage::Partial);
            assert!(compact.equal_regions.is_empty());
            assert_eq!(compact.unknown_regions.len(), 1);
            assert!(compact.render_text(&full).contains("unresolved call"));
        } else {
            assert_eq!(compact.coverage.result_comparison, Coverage::Complete);
            assert_eq!(compact.equal_regions.len(), 1);
            assert!(compact.unknown_regions.is_empty());
            assert!(
                full.analysis
                    .after
                    .as_ref()
                    .unwrap()
                    .unknown_boundaries
                    .is_empty()
            );
            assert!(!compact.render_text(&full).contains("Equal"));
        }
        compact.validate_with_full(&full).unwrap();
    }
}

#[test]
fn independently_authored_guard_and_status_cases() {
    let (full, compact) = reports(
        include_str!("ifds/function_result_cases/guard_added/before.ts"),
        include_str!("ifds/function_result_cases/guard_added/after.ts"),
        FunctionEntry::default(),
        FunctionEntry::default(),
    );
    assert_eq!(full.comparison.regions.len(), 3);
    assert_region(
        &full,
        &[(0, true), (1, true)],
        "\"ok\"",
        "\"ok\"",
        ResultAssessment::Equal,
    );
    assert_region(
        &full,
        &[(0, true), (1, false)],
        "\"ok\"",
        "\"skip\"",
        ResultAssessment::Different,
    );
    assert_region(
        &full,
        &[(0, false)],
        "\"skip\"",
        "\"skip\"",
        ResultAssessment::Equal,
    );
    assert_eq!(compact.findings.len(), 1);
    assert_eq!(compact.coverage.result_comparison, Coverage::Complete);
    compact.validate_with_full(&full).unwrap();

    let (full, compact) = reports(
        include_str!("ifds/function_result_cases/status/before.ts"),
        include_str!("ifds/function_result_cases/status/after.ts"),
        FunctionEntry::default(),
        FunctionEntry::default(),
    );
    assert_region(
        &full,
        &[(0, true), (1, false), (2, true), (3, false)],
        "\"ok\"",
        "\"pending\"",
        ResultAssessment::Different,
    );
    assert_eq!(compact.findings.len(), 1);
    assert_eq!(
        compact.findings[0].context.values,
        BTreeMap::from([(0, true), (1, false)])
    );
    assert!(
        compact
            .render_text(&full)
            .contains("\"pending\" | !ready | !ready || !approved")
    );
}

#[test]
fn independent_result_shapes_and_uncertainty() {
    let empty = FunctionEntry::default();
    let cases = [
        (
            "function value() { return 1; }",
            "function value() { return 2; }",
            vec![(vec![], "1", "2", ResultAssessment::Different)],
        ),
        (
            "function value(flag: boolean) { return 2; }",
            "function value(flag: boolean) { if (flag) return 1; return 2; }",
            vec![
                (vec![(0, true)], "2", "1", ResultAssessment::Different),
                (vec![(0, false)], "2", "2", ResultAssessment::Equal),
            ],
        ),
        (
            "function value(flag: boolean) { if (flag) return 1; }",
            "function value(flag: boolean) { if (flag) return 1; return 2; }",
            vec![
                (vec![(0, true)], "1", "1", ResultAssessment::Equal),
                (
                    vec![(0, false)],
                    "undefined",
                    "2",
                    ResultAssessment::Different,
                ),
            ],
        ),
        (
            "function value(flag: boolean) { if (flag) return \"yes\"; return \"no\"; }",
            "function value(flag: boolean) { if (!flag) return \"yes\"; return \"no\"; }",
            vec![
                (
                    vec![(0, true)],
                    "\"yes\"",
                    "\"no\"",
                    ResultAssessment::Different,
                ),
                (
                    vec![(0, false)],
                    "\"no\"",
                    "\"yes\"",
                    ResultAssessment::Different,
                ),
            ],
        ),
    ];
    for (before, after, expected) in cases {
        let (full, compact) = reports(before, after, empty.clone(), empty.clone());
        for (condition, before_result, after_result, relation) in expected {
            assert_region(&full, &condition, before_result, after_result, relation);
        }
        compact.validate_with_full(&full).unwrap();
    }
    let (full, compact) = reports(
        "function value(flag: boolean) { if (flag) return 1; mystery(); return 1; }",
        "function value(flag: boolean) { if (flag) return 2; mystery(); return 1; }",
        empty.clone(),
        empty,
    );
    assert_region(&full, &[(0, true)], "1", "2", ResultAssessment::Different);
    assert!(
        full.comparison
            .regions
            .iter()
            .any(|region| region.assessment == ResultAssessment::Unknown)
    );
    assert_eq!(compact.coverage.result_comparison, Coverage::Partial);
    assert!(compact.unchanged_choices.is_empty());
}

#[test]
fn no_common_inputs_keep_separate_flows() {
    let mut before = FunctionEntry::default();
    before.domains.insert(0, PrimitiveDomain::Boolean);
    before
        .known_values
        .insert(0, FunctionKnownValue::Boolean { value: true });
    let mut after = FunctionEntry::default();
    after.domains.insert(0, PrimitiveDomain::Boolean);
    after
        .known_values
        .insert(0, FunctionKnownValue::Boolean { value: false });
    let (full, compact) = reports(
        "function answer(flag: boolean) { if (flag) return \"old\"; return \"off\"; }",
        "function answer(flag: boolean) { if (flag) return \"new\"; return \"off\"; }",
        before,
        after,
    );
    assert_eq!(
        full.comparison.common_domain_status,
        CommonDomainStatus::Empty
    );
    assert!(full.comparison.regions.is_empty());
    assert_eq!(
        compact.before_flow[0].flow.result.as_deref(),
        Some("\"old\"")
    );
    assert_eq!(
        compact.after_flow[0].flow.result.as_deref(),
        Some("\"off\"")
    );
    assert!(compact.render_text(&full).contains("No common inputs"));
    compact.validate_with_full(&full).unwrap();
}

#[test]
fn dependency_and_control_cases_have_independent_relations() {
    let cases = [
        (
            "function same(flag: boolean) { if (flag) return 1; return 1; }",
            "function same(flag: boolean) { if (!flag) return 1; return 1; }",
            vec![],
            ResultAssessment::Equal,
            None,
        ),
        (
            "function value(flag: boolean) { return flag ? 1 : 2; }",
            "function value(flag: boolean) { if (flag) return 1; return 2; }",
            vec![(0, true)],
            ResultAssessment::Equal,
            None,
        ),
        (
            "function value(p: number, q: number) { let x = p; x = 3; return x; }",
            "function value(p: number, q: number) { let x = q; x = 3; return x; }",
            vec![],
            ResultAssessment::Equal,
            None,
        ),
        (
            "function value() { let x = 1; const y = x; x = 3; return y; }",
            "function value() { let x = 2; const y = x; x = 3; return y; }",
            vec![],
            ResultAssessment::Different,
            None,
        ),
        (
            "function echo(input: string) { return input; }",
            "function echo(input: string) { const copy = input; return copy; }",
            vec![],
            ResultAssessment::Equal,
            Some(PrimitiveDomain::String),
        ),
        (
            "function next(p: number) { return p + 1; }",
            "function next(p: number) { return p + 2; }",
            vec![],
            ResultAssessment::Changed,
            Some(PrimitiveDomain::Number),
        ),
        (
            "function outer() { function inner() { return 1; } return 3; }",
            "function outer() { function inner() { return 2; } return 3; }",
            vec![],
            ResultAssessment::Equal,
            None,
        ),
        (
            "function value(p: number) { return p; }",
            "function value(p: number) { return mystery(p); }",
            vec![],
            ResultAssessment::Unknown,
            Some(PrimitiveDomain::Number),
        ),
    ];
    for (before, after, condition, expected, domain) in cases {
        let mut entry = FunctionEntry::default();
        if let Some(domain) = domain {
            entry.domains.insert(0, domain);
        }
        let (full, compact) = reports(before, after, entry.clone(), entry);
        let condition: BTreeMap<_, _> = condition.into_iter().collect();
        assert!(
            full.comparison
                .regions
                .iter()
                .any(|region| region.region.values == condition && region.assessment == expected),
            "{before} -> {after}: {:#?}",
            full.comparison.regions
        );
        compact.validate_with_full(&full).unwrap();
    }
}

#[test]
fn changed_guard_versions_and_write_order() {
    let empty = FunctionEntry::default();
    let (full, compact) = reports(
        "function selected(flag: boolean) { let g = flag; if (g) return 1; return 2; }",
        "function selected(flag: boolean) { let g = flag; g = !g; if (g) return 1; return 2; }",
        empty.clone(),
        empty.clone(),
    );
    assert_eq!(full.comparison.regions.len(), 2);
    assert!(
        full.comparison
            .regions
            .iter()
            .all(|region| region.assessment == ResultAssessment::Different)
    );
    assert_eq!(compact.findings.len(), 1);
    assert_eq!(compact.findings[0].effects.len(), 2);
    assert_eq!(
        compact
            .render_text(&full)
            .matches("Return choice | Before | After")
            .count(),
        1
    );
    compact.validate_with_full(&full).unwrap();
    let (full, compact) = reports(
        "function priority(a: boolean, b: boolean) { let result = 0; if (a) result = 1; if (b) result = 2; return result; }",
        "function priority(a: boolean, b: boolean) { let result = 0; if (b) result = 2; if (a) result = 1; return result; }",
        empty.clone(),
        empty,
    );
    assert!(
        full.comparison
            .regions
            .iter()
            .any(
                |region| region.region.values == BTreeMap::from([(0, true), (1, true)])
                    && region.assessment == ResultAssessment::Different
            )
    );
    assert!(
        compact
            .render_text(&full)
            .contains("a && b: result (2) -> result (1) (Different)")
    );
    compact.validate_with_full(&full).unwrap();
}
