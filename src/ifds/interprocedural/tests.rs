use super::*;
use crate::ifds::adapters::typescript::TypeScriptBindingIndex;
use crate::ifds::project_metadata::*;
use crate::ifds::slicing::{SliceRequest, build_flow_slice};
use crate::ifds::solver::{
    IntraproceduralSolver, NoopSolverControl, Solver, SolverOutput, SolverRequest,
};

fn limits() -> AnalysisLimits {
    AnalysisLimits {
        time_ms: 30_000,
        memory_bytes: 128 * 1024 * 1024,
        processed_path_edges: 100_000,
        output_nodes: 10_000,
        witnesses_per_relation: 8,
    }
}

fn snapshot() -> SnapshotId {
    SnapshotId {
        side: SnapshotSide::Before,
        revision: "k018".into(),
        content_id: "k018-tree".into(),
    }
}

/// Targets are explicitly authored by each test, independently of the adapter.
fn fixture(
    main: &str,
    helpers: &[(&str, &str)],
    calls: &[(&str, &str, &str)],
) -> (
    ProjectProgram,
    ProcedureIr,
    TypeScriptBindingIndex,
    Vec<Diagnostic>,
) {
    let files: BTreeMap<_, _> = std::iter::once(("main.ts", main))
        .chain(helpers.iter().copied())
        .map(|(path, text)| {
            (
                path.to_owned(),
                MetadataFile {
                    text: text.into(),
                    content_id: path.into(),
                },
            )
        })
        .collect();
    let request = MetadataRequest {
        protocol_version: METADATA_PROTOCOL_VERSION,
        compiler_version: METADATA_COMPILER_VERSION.into(),
        snapshot: snapshot(),
        files,
        selected_paths: BTreeSet::from(["main.ts".into()]),
        options: ProjectOptions::default(),
    };
    let declarations: Vec<_> = request
        .files
        .iter()
        .flat_map(|(path, file)| {
            index_functions(path, &file.text)
                .unwrap()
                .into_iter()
                .map(move |f| DeclarationMetadata {
                    id: format!("{path}:{}", f.name.as_deref().unwrap()),
                    name: f.name.unwrap(),
                    kind: "function".into(),
                    span: f.span.clone(),
                    has_body: true,
                    implementation_span: Some(f.span),
                    declaration_only: false,
                    excluded: false,
                })
        })
        .collect();
    let calls = calls
        .iter()
        .map(|(path, expression, target)| {
            let text = &request.files[*path].text;
            let start = text.find(expression).unwrap() as u64;
            let name = expression.split('(').next().unwrap();
            let span = SourceSpan {
                path: (*path).into(),
                byte_start: start,
                byte_end: start + expression.len() as u64,
                start_line: 1,
                end_line: 1,
            };
            CallMetadata {
                callee_span: SourceSpan {
                    byte_end: start + name.len() as u64,
                    ..span.clone()
                },
                span,
                callee: name.into(),
                status: if target.is_empty() {
                    ResolutionStatus::UnsupportedDispatch
                } else {
                    ResolutionStatus::ResolvedBody
                },
                target: (!target.is_empty()).then(|| (*target).into()),
                candidates: Vec::new(),
                external_package: None,
                signature: None,
                chain: Vec::new(),
                reason: "authored test target".into(),
            }
        })
        .collect();
    let metadata = ProjectMetadata {
        protocol_version: METADATA_PROTOCOL_VERSION,
        compiler_version: METADATA_COMPILER_VERSION.into(),
        snapshot: snapshot(),
        tsconfig: "tsconfig.json".into(),
        configuration_files: BTreeMap::new(),
        platform: "web".into(),
        selected_paths: request.selected_paths.clone(),
        excluded_paths: BTreeSet::new(),
        runtime_aliases: BTreeMap::new(),
        compiler_options: serde_json::json!({}),
        declarations,
        calls,
        imports: Vec::new(),
        diagnostics: Vec::new(),
    };
    let program = ProjectProgram::from_metadata(&request, &metadata, "main.ts").unwrap();
    let index = index_bindings(snapshot(), "main.ts", main).unwrap();
    let function = index_functions("main.ts", main)
        .unwrap()
        .into_iter()
        .find(|f| f.name.as_deref() == Some("main"))
        .unwrap();
    let (procedure, diagnostics) = lower_function_procedure(main, &index, &function.span).unwrap();
    (program, procedure, index, diagnostics)
}

fn solve(prepared: &PreparedProcedure) -> SolverOutput {
    IntraproceduralSolver::new()
        .solve(
            SolverRequest {
                procedure: &prepared.procedure,
                entry_facts: seed_entry_facts(&prepared.procedure),
                limits: limits(),
            },
            &NoopSolverControl,
        )
        .unwrap()
}

fn binding(index: &TypeScriptBindingIndex, name: &str) -> BindingId {
    index
        .bindings
        .iter()
        .find(|b| b.name == name)
        .unwrap()
        .id
        .clone()
}
fn write_origin(procedure: &ProcedureIr, binding: &BindingId) -> Source {
    procedure
        .nodes
        .values()
        .find_map(|n| match &n.operation {
            Operation::Write {
                target: Place::Binding(b),
                definition,
                ..
            } if b == binding => Some(Source::Write(definition.clone())),
            _ => None,
        })
        .unwrap()
}
fn origins(solved: &SolverOutput, at: &NodeId, place: &Place) -> BTreeSet<Source> {
    solved
        .facts_at
        .get(at)
        .into_iter()
        .flatten()
        .filter_map(|f| match f {
            Fact::Origin { place: p, source } if p == place => Some(source.clone()),
            _ => None,
        })
        .collect()
}
fn return_node(procedure: &ProcedureIr) -> &IrNode {
    procedure
        .nodes
        .values()
        .find(|n| matches!(n.operation, Operation::Return { .. }))
        .unwrap()
}
fn prepared(
    f: &(
        ProjectProgram,
        ProcedureIr,
        TypeScriptBindingIndex,
        Vec<Diagnostic>,
    ),
) -> PreparedProcedure {
    prepare(
        f.1.clone(),
        f.3.clone(),
        Some(&f.0),
        &limits(),
        &NoopSolverControl,
    )
    .unwrap()
}

#[test]
fn ifds_k018_argument_return_chain() {
    let f = fixture(
        "function main(input: string) { const result = helper(input); return result; }",
        &[(
            "helper.ts",
            "export function helper(value: string) { return value + '!'; }",
        )],
        &[("main.ts", "helper(input)", "helper.ts:helper")],
    );
    let p = prepared(&f);
    let solved = solve(&p);
    let origin = Source::FunctionInput(binding(&f.2, "input"));
    assert!(
        origins(
            &solved,
            &return_node(&p.procedure).id,
            &Place::Binding(binding(&f.2, "result"))
        )
        .contains(&origin)
    );
    let parameter = p
        .procedure
        .nodes
        .values()
        .find_map(|n| match &n.operation {
            Operation::CallBind {
                boundary: CallBindKind::Argument,
                target,
                ..
            } => Some(target.clone()),
            _ => None,
        })
        .unwrap();
    let returned = p
        .procedure
        .nodes
        .values()
        .find(|n| {
            matches!(
                n.operation,
                Operation::CallBind {
                    boundary: CallBindKind::Return,
                    ..
                }
            )
        })
        .unwrap();
    assert!(origins(&solved, &returned.id, &parameter).contains(&origin));
    let tabulation = p.tabulation.as_ref().unwrap();
    assert!(
        tabulation
            .summaries
            .values()
            .flatten()
            .any(|exit| matches!(&exit.fact, Fact::Origin { source, .. } if source == &origin))
    );
    let mut slice = build_flow_slice(SliceRequest {
        procedure: &p.procedure,
        solved: &solved,
        selected_binding: &binding(&f.2, "input"),
        declared_scope: "function main",
    })
    .unwrap();
    p.annotate_slice(&mut slice);
    assert!(
        slice
            .graph
            .edges
            .iter()
            .any(|e| e.relation == RelationKind::Argument)
    );
    assert!(
        slice
            .graph
            .edges
            .iter()
            .any(|e| e.relation == RelationKind::Return)
    );
    assert!(slice.graph.nodes.iter().any(|n| n.span.path == "helper.ts"));
    assert!(slice.witnesses.iter().any(|w| !w.call_contexts.is_empty()));
}

#[test]
fn ifds_k018_two_calls_no_leak() {
    let f = fixture(
        "function main() { let language = 'en'; let first = decorate(language); let second = decorate('de'); return first + second; }",
        &[(
            "helper.ts",
            "export function decorate(value: string) { return value + '!'; }",
        )],
        &[
            ("main.ts", "decorate(language)", "helper.ts:decorate"),
            ("main.ts", "decorate('de')", "helper.ts:decorate"),
        ],
    );
    let p = prepared(&f);
    let solved = solve(&p);
    let language = binding(&f.2, "language");
    let origin = write_origin(&f.1, &language);
    let at = &return_node(&p.procedure).id;
    assert!(origins(&solved, at, &Place::Binding(binding(&f.2, "first"))).contains(&origin));
    assert!(!origins(&solved, at, &Place::Binding(binding(&f.2, "second"))).contains(&origin));
    let mut slice = build_flow_slice(SliceRequest {
        procedure: &p.procedure,
        solved: &solved,
        selected_binding: &binding(&f.2, "second"),
        declared_scope: "function main",
    })
    .unwrap();
    p.annotate_slice(&mut slice);
    let language_write = f.1.nodes.values().find(|n| matches!(&n.operation, Operation::Write { target: Place::Binding(b), .. } if b == &language)).unwrap();
    assert!(!slice.graph.nodes.iter().any(|n| n.id == language_write.id));
    let first_context = p.contexts.values().min().unwrap().clone();
    assert!(
        slice
            .witnesses
            .iter()
            .all(|w| !w.call_contexts.contains(&first_context))
    );
    // Native summaries themselves must keep A's entry origin out of B's result.
    let t = p.tabulation.as_ref().unwrap();
    let second_read = p.procedure.nodes.values().find(|n| matches!(&n.operation, Operation::Read { source: Place::Binding(b), .. } if b == &binding(&f.2, "second"))).unwrap();
    assert!(!t.records.iter().any(|r| r.node.local == second_read.id.local && matches!(&r.fact, Fact::Origin { place: Place::Binding(b), source } if b == &binding(&f.2, "second") && source == &origin)));
}

#[test]
fn ifds_k018_scalar_parameter_reassignment() {
    let f = fixture(
        "function main() { let language = 'en'; const result = replace(language); const saved = language; language = result; return saved + language; }",
        &[(
            "helper.ts",
            "export function replace(value: string) { value = 'fr'; return value; }",
        )],
        &[("main.ts", "replace(language)", "helper.ts:replace")],
    );
    let p = prepared(&f);
    let solved = solve(&p);
    let language = binding(&f.2, "language");
    let origin = write_origin(&f.1, &language);
    let at = &return_node(&p.procedure).id;
    assert!(origins(&solved, at, &Place::Binding(binding(&f.2, "saved"))).contains(&origin));
    assert!(!origins(&solved, at, &Place::Binding(language)).contains(&origin));
    assert!(!origins(&solved, at, &Place::Binding(binding(&f.2, "result"))).contains(&origin));
    let rebound = p.procedure.nodes.values().find(|n| matches!(&n.operation, Operation::Write { target: Place::Binding(b), .. } if b == &binding(&f.2, "language")) && n.span.as_ref().is_some_and(|s| s.byte_start == f.0.sources["main.ts"].find("language = result").unwrap() as u64)).unwrap();
    assert!(matches!(rebound.operation, Operation::Write { .. }));
}

#[test]
fn ifds_k018_unaffected_caller_locals() {
    let f = fixture(
        "function main() { let language = 'en'; const ignored = helper(); return language; }",
        &[(
            "helper.ts",
            "export function helper() { let unrelated = 'fr'; return unrelated; }",
        )],
        &[("main.ts", "helper()", "helper.ts:helper")],
    );
    let p = prepared(&f);
    let solved = solve(&p);
    let language = binding(&f.2, "language");
    let origin = write_origin(&f.1, &language);
    assert!(
        origins(
            &solved,
            &return_node(&p.procedure).id,
            &Place::Binding(language)
        )
        .contains(&origin)
    );
    assert!(p.diagnostics.is_empty());
    assert!(solved.diagnostics.is_empty());
}

#[test]
fn ifds_k018_summary_reuse() {
    let f = fixture(
        "function main(input: string) { const first = helper(input); const second = helper(input); return first + second; }",
        &[(
            "helper.ts",
            "export function helper(value: string) { return value; }",
        )],
        &[("main.ts", "helper(input)", "helper.ts:helper")],
    );
    // The fixture's two identical expressions require two explicit source spans.
    let mut program = f.0.clone();
    let metadata = program.metadata.as_mut().unwrap();
    let mut second = metadata.calls[0].clone();
    let start = program.sources["main.ts"].rfind("helper(input)").unwrap() as u64;
    second.span.byte_start = start;
    second.span.byte_end = start + "helper(input)".len() as u64;
    second.callee_span.byte_start = start;
    second.callee_span.byte_end = start + 6;
    metadata.calls.push(second);
    let p = prepare(
        f.1.clone(),
        f.3.clone(),
        Some(&program),
        &limits(),
        &NoopSolverControl,
    )
    .unwrap();
    let t = p.tabulation.as_ref().unwrap();
    let helper = &program.declarations["helper.ts:helper"];
    let origin = Source::FunctionInput(binding(&f.2, "input"));
    let keys: Vec<_> = t
        .summaries
        .keys()
        .filter(|(p, fact)| {
            p == helper && matches!(fact, Fact::Origin { source, .. } if source == &origin)
        })
        .collect();
    assert_eq!(keys.len(), 1);
    let key = keys[0];
    assert_eq!(
        t.records
            .iter()
            .filter(|r| &r.procedure == helper
                && r.node == program.procedures[helper].entry
                && r.entry_fact == key.1)
            .count(),
        1
    );
    assert!(t.summary_applications > 1);
    let contexts: BTreeSet<_> = p.contexts.values().collect();
    assert_eq!(contexts.len(), 2);
    let solved = solve(&p);
    for name in ["first", "second"] {
        assert!(
            origins(
                &solved,
                &return_node(&p.procedure).id,
                &Place::Binding(binding(&f.2, name))
            )
            .contains(&origin)
        );
    }
}

#[test]
fn ifds_k018_zero_and_new_sources() {
    let f = fixture(
        "function main() { const result = fresh(); return result; }",
        &[("helper.ts", "export function fresh() { return 'de'; }")],
        &[("main.ts", "fresh()", "helper.ts:fresh")],
    );
    let p = prepared(&f);
    let solved = solve(&p);
    let fresh_origin = p
        .procedure
        .nodes
        .values()
        .find_map(|n| match &n.operation {
            Operation::Literal {
                definition, raw, ..
            } if raw == "'de'" => Some(Source::Write(definition.clone())),
            _ => None,
        })
        .unwrap();
    assert!(
        origins(
            &solved,
            &return_node(&p.procedure).id,
            &Place::Binding(binding(&f.2, "result"))
        )
        .contains(&fresh_origin)
    );
    let t = p.tabulation.as_ref().unwrap();
    assert!(
        t.summaries
            .iter()
            .any(|((_, entry), exits)| *entry == Fact::Zero
                && exits.iter().any(|e| matches!(&e.fact, Fact::Origin { .. })))
    );
}

#[test]
fn ifds_k018_unknown_target() {
    let f = fixture(
        "function main(input: string) { let language = 'en'; const result = dispatch(input); return language + result; }",
        &[],
        &[("main.ts", "dispatch(input)", "")],
    );
    let p = prepared(&f);
    let solved = solve(&p);
    assert!(p.contexts.is_empty());
    assert!(solved.diagnostics.iter().any(|d| matches!(
        d.code,
        DiagnosticCode::UnsupportedSyntax | DiagnosticCode::UnresolvedCall
    )));
    assert!(
        origins(
            &solved,
            &return_node(&p.procedure).id,
            &Place::Binding(binding(&f.2, "language"))
        )
        .is_empty()
    );
    assert!(
        !origins(
            &solved,
            &return_node(&p.procedure).id,
            &Place::Binding(binding(&f.2, "result"))
        )
        .contains(&Source::FunctionInput(binding(&f.2, "input")))
    );
}

#[test]
fn ifds_k018_nested_calls_other_files() {
    let f = fixture(
        "function main(input: string) { const result = outer(inner(input)); return result; }",
        &[
            (
                "outer.ts",
                "export function outer(value: string) { return inner(value); }",
            ),
            (
                "inner.ts",
                "export function inner(value: string) { return value; }",
            ),
        ],
        &[
            ("main.ts", "outer(inner(input))", "outer.ts:outer"),
            ("main.ts", "inner(input)", "inner.ts:inner"),
            ("outer.ts", "inner(value)", "inner.ts:inner"),
        ],
    );
    let p = prepared(&f);
    let solved = solve(&p);
    assert!(p.contexts.values().any(|c| c.len() == 2));
    assert!(
        origins(
            &solved,
            &return_node(&p.procedure).id,
            &Place::Binding(binding(&f.2, "result"))
        )
        .contains(&Source::FunctionInput(binding(&f.2, "input")))
    );
    assert!(solved.diagnostics.is_empty());
}

#[test]
fn ifds_k018_parameter_capture_is_boundary() {
    let f = fixture(
        "function main() { let language = 'en'; function replace() { language = 'fr'; return language; } const result = replace(); return language; }",
        &[],
        &[("main.ts", "replace()", "main.ts:replace")],
    );
    let p = prepared(&f);
    assert!(p.contexts.is_empty());
    assert!(
        p.procedure
            .nodes
            .values()
            .any(|n| matches!(n.operation, Operation::UnknownEffect { .. }))
    );
}

#[test]
fn ifds_k018_limits_preserve_boundary() {
    let f = fixture(
        "function main(input: string) { const result = helper(input); return result; }",
        &[(
            "helper.ts",
            "export function helper(value: string) { return value; }",
        )],
        &[("main.ts", "helper(input)", "helper.ts:helper")],
    );
    let limits = AnalysisLimits {
        processed_path_edges: 1,
        ..limits()
    };
    let p = prepare(f.1, f.3, Some(&f.0), &limits, &NoopSolverControl).unwrap();
    assert!(matches!(
        p.tabulation.as_ref().unwrap().termination,
        SolverTermination::LimitExceeded(_)
    ));
    assert!(
        p.diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::AnalysisBudgetExceeded)
    );
    assert!(p.contexts.is_empty());
}

struct AuthoredMetadata {
    before: ProjectMetadata,
    after: ProjectMetadata,
}
impl MetadataProvider for AuthoredMetadata {
    fn metadata(&self, request: &MetadataRequest) -> Result<ProjectMetadata, MetadataError> {
        let mut metadata = if request.snapshot.side == SnapshotSide::Before {
            self.before.clone()
        } else {
            self.after.clone()
        };
        metadata.snapshot = request.snapshot.clone();
        metadata.platform = request.options.platform.clone();
        metadata.selected_paths = request.selected_paths.clone();
        metadata.excluded_paths = request.options.excluded_paths.clone();
        metadata.runtime_aliases = request.options.runtime_aliases.clone();
        metadata.configuration_files.insert(
            "tsconfig.json".into(),
            request.files["tsconfig.json"].clone(),
        );
        let ids: BTreeMap<_, _> = metadata
            .declarations
            .iter()
            .map(|d| (d.id.clone(), declaration_identity(&d.span, &d.kind)))
            .collect();
        for declaration in &mut metadata.declarations {
            declaration.id = ids[&declaration.id].clone();
        }
        for call in &mut metadata.calls {
            call.target = call.target.as_ref().map(|id| ids[id].clone());
            call.candidates = call.target.iter().cloned().collect();
        }
        Ok(metadata)
    }
}

#[test]
fn ifds_k018_both_metadata_apis_helper_change() {
    use crate::ifds::*;
    let main = "import { helper } from './helper'; function main(input: string) { let language = helper(input); return language; }";
    let before_helper = "export function helper(value: string) { return value + '!'; }";
    let after_helper = "export function helper(value: string) { return value + '?'; }";
    let before_fixture = fixture(
        main,
        &[("helper.ts", before_helper)],
        &[("main.ts", "helper(input)", "helper.ts:helper")],
    );
    let after_fixture = fixture(
        main,
        &[("helper.ts", after_helper)],
        &[("main.ts", "helper(input)", "helper.ts:helper")],
    );
    let metadata = AuthoredMetadata {
        before: before_fixture.0.metadata.unwrap(),
        after: after_fixture.0.metadata.unwrap(),
    };
    let before = SnapshotHandle {
        id: snapshot(),
        repository_id: "k018".into(),
    };
    let after = SnapshotHandle {
        id: SnapshotId {
            side: SnapshotSide::After,
            revision: "k018-after".into(),
            content_id: "k018-after-tree".into(),
        },
        repository_id: "k018".into(),
    };
    let snapshots = InMemorySnapshotProvider::new(
        [
            (&before, before_helper, "helper-before"),
            (&after, after_helper, "helper-after"),
        ]
        .into_iter()
        .map(|(handle, helper, blob)| InMemorySnapshot {
            handle: handle.clone(),
            files: BTreeMap::from([
                (
                    "main.ts".into(),
                    ("main-unchanged".into(), main.as_bytes().to_vec()),
                ),
                (
                    "helper.ts".into(),
                    (blob.into(), helper.as_bytes().to_vec()),
                ),
                ("tsconfig.json".into(), ("config".into(), b"{}".to_vec())),
            ]),
        }),
    );
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
        version: "k018".into(),
        capabilities: BTreeSet::new(),
    };
    let environment = AnalysisEnvironment {
        capabilities: capabilities.clone(),
        summaries: BTreeSet::new(),
    };
    let selected = before_fixture
        .2
        .bindings
        .iter()
        .find(|b| b.name == "language")
        .unwrap();
    let query = VariableFlowQuery {
        before: before.clone(),
        after: after.clone(),
        diff: diff.clone(),
        selected_binding: BindingSelector {
            snapshot: before.id.clone(),
            declaration: selected.declaration.clone(),
            expected_name: Some("language".into()),
            expected_enclosing_symbol: selected.enclosing_symbol.clone(),
        },
        counterpart: None,
        entry: EntryPoint::ContainingFunction,
        capabilities: capabilities.clone(),
        summaries: BTreeSet::new(),
        limits: limits(),
    };
    let options = ProjectOptions {
        tsconfig: Some("tsconfig.json".into()),
        platform: "web".into(),
        ..ProjectOptions::default()
    };
    let pair = ProjectPairOptions {
        before: options.clone(),
        after: options,
    };
    let report = analyze_variable_flow_with_metadata(
        query.clone(),
        &snapshots,
        (&environment, &environment),
        pair.clone(),
        &metadata,
    )
    .unwrap();
    assert!(
        report
            .analysis
            .before_graph
            .nodes
            .iter()
            .any(|n| n.span.path == "helper.ts" && n.operation.contains("'!'"))
    );
    assert!(
        report
            .analysis
            .after_graph
            .nodes
            .iter()
            .any(|n| n.span.path == "helper.ts" && n.operation.contains("'?'"))
    );
    assert!(
        report
            .analysis
            .witnesses
            .iter()
            .any(|w| !w.call_contexts.is_empty())
    );
    assert!(!report.analysis.deltas.is_empty());
    let local = analyze_variable_flow(query, &snapshots, &environment, &environment).unwrap();
    assert!(
        local
            .before_graph
            .nodes
            .iter()
            .all(|n| n.span.path == "main.ts")
    );
    assert!(
        local
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::UnsupportedSyntax)
    );
    let function = index_functions("main.ts", main).unwrap().remove(0);
    let entry = FunctionEntry {
        domains: BTreeMap::from([(0, PrimitiveDomain::String)]),
        ..FunctionEntry::default()
    };
    let query = FunctionResultQuery {
        before: before.clone(),
        after,
        diff,
        selected_function: FunctionSelector {
            snapshot: before.id,
            declaration: function.span,
            expected_name: function.name,
            expected_enclosing_symbol: function.owner,
        },
        counterpart: None,
        before_entry: entry.clone(),
        after_entry: entry,
        capabilities,
        summaries: BTreeSet::new(),
        limits: limits(),
    };
    let report = analyze_function_result_with_metadata(
        query,
        &snapshots,
        (&environment, &environment),
        pair,
        &metadata,
    )
    .unwrap();
    fn has_literal(dep: &ResultDependency, literal: &str) -> bool {
        dep.literal
            == Some(FunctionKnownValue::String {
                value: literal.into(),
            })
            || dep.inputs.iter().any(|d| has_literal(d, literal))
    }
    for (side, literal) in [
        (&report.analysis.analysis.before, "!"),
        (&report.analysis.analysis.after, "?"),
    ] {
        let side = side.as_ref().unwrap();
        assert_eq!(side.coverage, Coverage::Complete, "{:?}", side.diagnostics);
        assert_eq!(side.observations.len(), 1);
        let ResultValue::Expression { dependency, .. } = &side.observations[0].value else {
            panic!("expected expression")
        };
        assert!(has_literal(dependency, literal));
        fn context(dep: &ResultDependency) -> bool {
            !dep.call_context.is_empty() || dep.inputs.iter().any(context)
        }
        assert!(context(dependency));
    }
    let json = serde_json::to_string(&report.analysis).unwrap();
    FunctionResultReport::from_json(json.as_bytes()).unwrap();
}

#[test]
fn ifds_k018_missing_arguments_and_undefined_returns() {
    for helper in [
        "export function helper(value: string) { return value; }",
        "export function helper(value: string) { return; }",
        "export function helper(value: string) { const unused = 'fr'; }",
    ] {
        let f = fixture(
            "function main() { const result = helper(); return result; }",
            &[("helper.ts", helper)],
            &[("main.ts", "helper()", "helper.ts:helper")],
        );
        let p = prepared(&f);
        let solved = solve(&p);
        let sources = origins(
            &solved,
            &return_node(&p.procedure).id,
            &Place::Binding(binding(&f.2, "result")),
        );
        assert!(sources.iter().any(|source| p.procedure.nodes.values().any(|node| matches!((&node.operation, source), (Operation::Literal { literal_kind: LiteralKind::Undefined, definition, .. }, Source::Write(d)) if definition == d))));
        assert!(
            p.tabulation
                .as_ref()
                .unwrap()
                .summaries
                .values()
                .flatten()
                .any(|exit| matches!(exit.fact, Fact::Origin { .. }))
        );
    }
}

#[test]
fn ifds_k018_supported_prefix_unknown_callee_effects() {
    let f = fixture(
        "function main(input: string) { let language = 'en'; const result = helper(input); return language + result; }",
        &[(
            "helper.ts",
            "export function helper(value: string) { const saved = value; external(); return saved; }",
        )],
        &[
            ("main.ts", "helper(input)", "helper.ts:helper"),
            ("helper.ts", "external()", ""),
        ],
    );
    let p = prepared(&f);
    let solved = solve(&p);
    assert!(!p.contexts.is_empty());
    assert!(
        origins(
            &solved,
            &return_node(&p.procedure).id,
            &Place::Binding(binding(&f.2, "language"))
        )
        .is_empty()
    );
    assert!(
        origins(
            &solved,
            &return_node(&p.procedure).id,
            &Place::Binding(binding(&f.2, "result"))
        )
        .contains(&Source::FunctionInput(binding(&f.2, "input")))
    );
    assert!(!solved.unknown_frontiers.is_empty());
}

#[test]
fn ifds_k018_branch_return_contexts() {
    let f = fixture(
        "function main() { const result = choose('en', false); return result; }",
        &[(
            "helper.ts",
            "export function choose(value: string, useFallback: boolean) { if (useFallback) { return 'de'; } return value; }",
        )],
        &[("main.ts", "choose('en', false)", "helper.ts:choose")],
    );
    let p = prepared(&f);
    let paths = super::super::branches::BranchPaths::build(
        &p.procedure,
        seed_entry_facts(&p.procedure),
        4096,
    );
    let return_site = return_node(&p.procedure);
    let result = Place::Binding(binding(&f.2, "result"));
    let forbidden = p
        .procedure
        .nodes
        .values()
        .find_map(|n| match &n.operation {
            Operation::Literal {
                raw, definition, ..
            } if raw == "'de'" => Some(Source::Write(definition.clone())),
            _ => None,
        })
        .unwrap();
    assert!(
        paths.at[&return_site.id]
            .iter()
            .all(|state| !state.facts.contains(&Fact::Origin {
                place: result.clone(),
                source: forbidden.clone()
            }))
    );
    let permitted = p
        .procedure
        .nodes
        .values()
        .find_map(|n| match &n.operation {
            Operation::Literal {
                raw, definition, ..
            } if raw == "'en'" => Some(Source::Write(definition.clone())),
            _ => None,
        })
        .unwrap();
    assert!(
        paths.at[&return_site.id]
            .iter()
            .all(|state| state.facts.contains(&Fact::Origin {
                place: result.clone(),
                source: permitted.clone()
            }))
    );
}

#[test]
fn ifds_k018_constant_return_has_no_argument_dependency() {
    let f = fixture(
        "function main(input: string) { const result = fresh(input); return result; }",
        &[(
            "helper.ts",
            "export function fresh(value: string) { return 'de'; }",
        )],
        &[("main.ts", "fresh(input)", "helper.ts:fresh")],
    );
    let p = prepared(&f);
    let input = Source::FunctionInput(binding(&f.2, "input"));
    assert!(
        !p.tabulation.as_ref().unwrap().summaries.iter().any(
            |((procedure, entry), exits)| procedure == &f.1.id
                && matches!(entry, Fact::Origin { source, .. } if source == &input)
                && exits
                    .iter()
                    .any(|exit| matches!(exit.fact, Fact::Origin { .. }))
        )
    );
    let solved = solve(&p);
    assert!(
        !origins(
            &solved,
            &return_node(&p.procedure).id,
            &Place::Binding(binding(&f.2, "result"))
        )
        .contains(&input)
    );
}

#[test]
fn ifds_k018_heap_read_keeps_open_boundary() {
    let f = fixture(
        "function main(input: { value: string }) { let language = 'en'; const result = helper(input); return language + result; }",
        &[(
            "helper.ts",
            "export function helper(value: { value: string }) { return value.value; }",
        )],
        &[("main.ts", "helper(input)", "helper.ts:helper")],
    );
    let p = prepared(&f);
    let solved = solve(&p);
    assert!(p.procedure.nodes.values().any(|node| matches!(&node.operation, Operation::UnknownEffect { description, .. } if description.contains("getter effects"))));
    assert!(!solved.unknown_frontiers.is_empty());
    assert!(
        origins(
            &solved,
            &return_node(&p.procedure).id,
            &Place::Binding(binding(&f.2, "language"))
        )
        .is_empty()
    );
    assert!(
        !origins(
            &solved,
            &return_node(&p.procedure).id,
            &Place::Binding(binding(&f.2, "result"))
        )
        .contains(&Source::FunctionInput(binding(&f.2, "input")))
    );
    let mut slice = build_flow_slice(SliceRequest {
        procedure: &p.procedure,
        solved: &solved,
        selected_binding: &binding(&f.2, "language"),
        declared_scope: "function main",
    })
    .unwrap();
    p.annotate_slice(&mut slice);
    assert_eq!(slice.completeness, Completeness::Partial);
    assert!(!slice.extent.value_lifecycle_closed);
    assert!(
        slice
            .path_endings
            .iter()
            .any(|ending| ending.kind == PathEndingKind::UnknownBoundary)
    );
    assert!(
        slice
            .path_endings
            .iter()
            .all(|ending| ending.kind != PathEndingKind::NoFurtherUse)
    );
}
