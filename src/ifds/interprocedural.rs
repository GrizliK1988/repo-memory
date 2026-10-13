//! Matched IFDS tabulation and bounded, call-specific report evidence expansion.
//!
//! Solver records contain static entry facts, never runtime stacks. The report
//! adapter expands nonrecursive summary bodies into distinct static namespaces
//! so the existing branch validator can check each actual/formal context.

use super::adapters::typescript::{index_bindings, index_functions, lower_function_procedure};
use super::ir::*;
use super::model::*;
use super::project_metadata::{MetadataRequest, ProjectMetadata, ResolutionStatus};
use super::provenance::{seed_entry_facts, transfer_operation};
use super::solver::{SolverControl, SolverCounters, SolverTermination};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const NAMESPACE: u64 = 1 << 48;

#[derive(Debug, Clone)]
pub struct CallTarget {
    pub callee: ProcedureId,
    pub return_site: NodeId,
    pub affected_places: BTreeSet<Place>,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectProgram {
    pub procedures: BTreeMap<ProcedureId, ProcedureIr>,
    pub calls: BTreeMap<NodeId, CallTarget>,
    sources: BTreeMap<String, String>,
    metadata: Option<ProjectMetadata>,
    file_namespaces: BTreeMap<String, u64>,
    declarations: BTreeMap<String, ProcedureId>,
    immutable_bindings: BTreeSet<BindingId>,
    next_namespace: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntryRecord {
    pub procedure: ProcedureId,
    pub entry_fact: Fact,
    pub node: NodeId,
    pub fact: Fact,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExitFact {
    pub exit: NodeId,
    pub fact: Fact,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Waiter {
    caller: ProcedureId,
    caller_entry: Fact,
    call: NodeId,
    caller_fact: Fact,
    return_origins: bool,
}

#[derive(Debug, Clone)]
pub struct TabulationOutput {
    pub records: BTreeSet<EntryRecord>,
    pub summaries: BTreeMap<(ProcedureId, Fact), BTreeSet<ExitFact>>,
    pub waiting_callers: usize,
    pub summary_applications: u64,
    pub counters: SolverCounters,
    pub termination: SolverTermination,
}

fn place_of(fact: &Fact) -> Option<&Place> {
    match fact {
        Fact::Origin { place, .. } | Fact::LastWrite { place, .. } => Some(place),
        Fact::Zero => None,
    }
}

fn schedule(
    record: EntryRecord,
    records: &mut BTreeSet<EntryRecord>,
    queue: &mut VecDeque<EntryRecord>,
) {
    if records.insert(record.clone()) {
        queue.push_back(record);
    }
}

fn argument_facts(call: &Operation, callee: &ProcedureIr, fact: &Fact) -> BTreeSet<Fact> {
    let Operation::Call { arguments, .. } = call else {
        return BTreeSet::new();
    };
    // Every caller fact subscribes to normal completion through Zero. Bypass is
    // resumed only after a matching exit is found, including no-return callees.
    let mut output = BTreeSet::new();
    if *fact == Fact::Zero {
        output.insert(Fact::Zero);
        for parameter in &callee.parameters {
            output.insert(Fact::LastWrite {
                place: Place::Binding(parameter.binding.clone()),
                write: parameter.entry_definition.clone(),
            });
        }
    }
    if let Fact::Origin { place, source } = fact {
        for (argument, parameter) in arguments.iter().zip(&callee.parameters) {
            if place == argument {
                output.insert(Fact::Origin {
                    place: Place::Binding(parameter.binding.clone()),
                    source: source.clone(),
                });
            }
        }
    }
    output
}

fn resume(
    program: &ProjectProgram,
    waiter: &Waiter,
    exit: &ExitFact,
    records: &mut BTreeSet<EntryRecord>,
    queue: &mut VecDeque<EntryRecord>,
) {
    let caller = &program.procedures[&waiter.caller];
    let Operation::Call { result, .. } = &caller.nodes[&waiter.call].operation else {
        return;
    };
    let target = &program.calls[&waiter.call];
    let mut facts = BTreeSet::new();
    if exit.fact == Fact::Zero
        && place_of(&waiter.caller_fact).is_none_or(|place| {
            !target.affected_places.contains(place) && result.as_ref() != Some(place)
        })
    {
        facts.insert(waiter.caller_fact.clone());
    }
    if waiter.return_origins
        && let (Some(result), Fact::Origin { source, .. }) = (result, &exit.fact)
    {
        facts.insert(Fact::Origin {
            place: result.clone(),
            source: source.clone(),
        });
    }
    for fact in facts {
        schedule(
            EntryRecord {
                procedure: waiter.caller.clone(),
                entry_fact: waiter.caller_entry.clone(),
                node: target.return_site.clone(),
                fact,
            },
            records,
            queue,
        );
    }
}

/// IFDS entry-to-node records, waiting callers, and reusable entry/exit relations.
/// Summaries are data dependencies, not memoized runtime values.
pub fn tabulate(
    program: &ProjectProgram,
    root: &ProcedureId,
    entry_facts: BTreeSet<Fact>,
    limits: &AnalysisLimits,
    control: &dyn SolverControl,
) -> Result<TabulationOutput, String> {
    for procedure in program.procedures.values() {
        procedure.validate().map_err(|e| e.to_string())?;
    }
    let procedure = program
        .procedures
        .get(root)
        .ok_or("missing entry procedure")?;
    for (node, target) in &program.calls {
        if !program.procedures.contains_key(&target.callee)
            || !program
                .procedures
                .values()
                .any(|p| p.nodes.contains_key(node) && p.nodes.contains_key(&target.return_site))
        {
            return Err("invalid resolved call mapping".into());
        }
    }
    let mut records = BTreeSet::new();
    let mut queue = VecDeque::new();
    let mut summaries: BTreeMap<(ProcedureId, Fact), BTreeSet<ExitFact>> = BTreeMap::new();
    let mut waiters: BTreeMap<(ProcedureId, Fact), BTreeSet<Waiter>> = BTreeMap::new();
    for fact in entry_facts {
        schedule(
            EntryRecord {
                procedure: root.clone(),
                entry_fact: fact.clone(),
                node: procedure.entry.clone(),
                fact,
            },
            &mut records,
            &mut queue,
        );
    }
    let mut counters = SolverCounters::default();
    let mut applications = 0;
    let mut termination = SolverTermination::FixedPoint;
    while let Some(record) = queue.pop_front() {
        counters.discovered_states = records.len() as u64;
        counters.accounted_memory_bytes = (records.len() as u64)
            .saturating_mul(512)
            .saturating_add(
                waiters
                    .values()
                    .map(|v| v.len() as u64)
                    .sum::<u64>()
                    .saturating_mul(512),
            )
            .saturating_add(
                summaries
                    .values()
                    .map(|v| v.len() as u64)
                    .sum::<u64>()
                    .saturating_mul(256),
            );
        if let Some(reason) = control.checkpoint(&counters) {
            termination = SolverTermination::Cancelled(reason);
            break;
        }
        let exceeded = if control.elapsed_ms() >= limits.time_ms {
            Some((
                AnalysisLimitKind::Time,
                limits.time_ms,
                control.elapsed_ms(),
            ))
        } else if control.memory_bytes(&counters) >= limits.memory_bytes {
            Some((
                AnalysisLimitKind::Memory,
                limits.memory_bytes,
                control.memory_bytes(&counters),
            ))
        } else if counters.processed_path_edges >= limits.processed_path_edges {
            Some((
                AnalysisLimitKind::ProcessedPathEdges,
                limits.processed_path_edges,
                counters.processed_path_edges,
            ))
        } else {
            None
        };
        if let Some((kind, limit, observed)) = exceeded {
            termination = SolverTermination::LimitExceeded(LimitReason {
                kind,
                limit,
                observed,
            });
            break;
        }
        counters.processed_path_edges += 1;
        let procedure = &program.procedures[&record.procedure];
        let operation = &procedure.nodes[&record.node].operation;
        if let Some(target) = program.calls.get(&record.node) {
            let callee = &program.procedures[&target.callee];
            let mut entries: Vec<_> = argument_facts(operation, callee, &record.fact)
                .into_iter()
                .map(|fact| (fact, true))
                .collect();
            if record.fact != Fact::Zero {
                // A completion subscription is not an argument dependency on
                // sources created by the callee's Zero summary.
                entries.push((Fact::Zero, false));
            }
            for (entry_fact, return_origins) in entries {
                let key = (callee.id.clone(), entry_fact.clone());
                let waiter = Waiter {
                    caller: record.procedure.clone(),
                    caller_entry: record.entry_fact.clone(),
                    call: record.node.clone(),
                    caller_fact: record.fact.clone(),
                    return_origins,
                };
                if waiters
                    .entry(key.clone())
                    .or_default()
                    .insert(waiter.clone())
                    && let Some(exits) = summaries.get(&key)
                {
                    for exit in exits {
                        applications += 1;
                        resume(program, &waiter, exit, &mut records, &mut queue);
                    }
                }
                schedule(
                    EntryRecord {
                        procedure: callee.id.clone(),
                        entry_fact: entry_fact.clone(),
                        node: callee.entry.clone(),
                        fact: entry_fact,
                    },
                    &mut records,
                    &mut queue,
                );
            }
            continue;
        }
        if let Operation::Return { value } = operation {
            if record.fact == Fact::Zero
                || matches!(&record.fact, Fact::Origin { place, .. } if Some(place) == value.as_ref())
            {
                let key = (record.procedure.clone(), record.entry_fact.clone());
                let exit = ExitFact {
                    exit: record.node.clone(),
                    fact: record.fact.clone(),
                };
                if summaries
                    .entry(key.clone())
                    .or_default()
                    .insert(exit.clone())
                    && let Some(waiting) = waiters.get(&key)
                {
                    for waiter in waiting {
                        applications += 1;
                        resume(program, waiter, &exit, &mut records, &mut queue);
                    }
                }
            }
            continue;
        }
        let output = transfer_operation(operation, &record.fact).into_facts();
        for edge in procedure
            .edges
            .iter()
            .filter(|edge| edge.source == record.node)
        {
            if !matches!(
                edge.kind,
                EdgeKind::Normal | EdgeKind::Branch { .. } | EdgeKind::LoopBack
            ) {
                continue;
            }
            for fact in &output {
                schedule(
                    EntryRecord {
                        procedure: record.procedure.clone(),
                        entry_fact: record.entry_fact.clone(),
                        node: edge.target.clone(),
                        fact: fact.clone(),
                    },
                    &mut records,
                    &mut queue,
                );
            }
        }
    }
    counters.discovered_states = records.len() as u64;
    Ok(TabulationOutput {
        records,
        summaries,
        waiting_callers: waiters.values().map(BTreeSet::len).sum(),
        summary_applications: applications,
        counters,
        termination,
    })
}

// IDs are file-local before K018. Qualifying every identity, including sources,
// guards and loop members, avoids collisions when composing files/contexts.
fn remap<T: Serialize + serde::de::DeserializeOwned>(value: &T, namespace: u64) -> T {
    fn visit(value: &mut serde_json::Value, namespace: u64) {
        match value {
            serde_json::Value::Object(fields) => {
                if fields.contains_key("snapshot")
                    && fields.len() == 2
                    && let Some(local) = fields.get_mut("local")
                {
                    *local = serde_json::Value::from(
                        local.as_u64().expect("numeric IR identity") % NAMESPACE + namespace,
                    );
                    return;
                }
                for value in fields.values_mut() {
                    visit(value, namespace)
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    visit(value, namespace)
                }
            }
            _ => {}
        }
    }
    // IR node maps have non-string keys and cannot be serialized as JSON maps.
    let mut value = serde_json::to_value(value).expect("serializable IR fragment");
    visit(&mut value, namespace);
    serde_json::from_value(value).expect("identity remapping preserves schema")
}

fn remap_procedure(procedure: &ProcedureIr, namespace: u64) -> ProcedureIr {
    ProcedureIr {
        id: remap(&procedure.id, namespace),
        parameters: remap(&procedure.parameters, namespace),
        entry: remap(&procedure.entry, namespace),
        exits: remap(&procedure.exits, namespace),
        nodes: procedure
            .nodes
            .values()
            .map(|node| {
                let node: IrNode = remap(node, namespace);
                (node.id.clone(), node)
            })
            .collect(),
        edges: remap(&procedure.edges, namespace),
        loops: remap(&procedure.loops, namespace),
    }
}

fn next_id(procedure: &ProcedureIr) -> NodeId {
    NodeId::new(
        procedure.id.snapshot.clone(),
        procedure.nodes.keys().map(|id| id.local).max().unwrap_or(0) + 1,
    )
}

fn normal_edge(source: NodeId, target: NodeId) -> IrEdge {
    IrEdge {
        source,
        target,
        kind: EdgeKind::Normal,
        construct: None,
        assumption: None,
    }
}

fn normalize_returns(procedure: &mut ProcedureIr) {
    // Fallthrough and bare return have the ordinary JavaScript undefined value.
    let bare: Vec<_> = procedure
        .nodes
        .values()
        .filter(|n| matches!(n.operation, Operation::Return { value: None }))
        .map(|n| n.id.clone())
        .collect();
    let fallthrough: Vec<_> = procedure
        .edges
        .iter()
        .filter(|e| {
            procedure.exits.contains(&e.target)
                && !matches!(
                    procedure.nodes[&e.source].operation,
                    Operation::Return { .. }
                )
        })
        .cloned()
        .collect();
    for edge in fallthrough {
        let id = next_id(procedure);
        let span = procedure.nodes[&edge.target].span.clone();
        procedure.nodes.insert(
            id.clone(),
            IrNode {
                id: id.clone(),
                span,
                operation: Operation::Return { value: None },
            },
        );
        procedure.edges.remove(&edge);
        procedure.edges.insert(IrEdge {
            target: id.clone(),
            ..edge.clone()
        });
        procedure.edges.insert(normal_edge(id.clone(), edge.target));
    }
    let mut returns = bare;
    returns.extend(
        procedure
            .nodes
            .values()
            .filter(|n| matches!(n.operation, Operation::Return { value: None }))
            .map(|n| n.id.clone()),
    );
    returns.sort();
    returns.dedup();
    for ret in returns {
        let id = next_id(procedure);
        let place = Place::Temporary(id.clone());
        procedure.nodes.insert(
            id.clone(),
            IrNode {
                id: id.clone(),
                span: procedure.nodes[&ret].span.clone(),
                operation: Operation::Literal {
                    definition: DefinitionId::new(id.snapshot.clone(), id.local),
                    literal_kind: LiteralKind::Undefined,
                    raw: "undefined".into(),
                    cooked: None,
                    result: place.clone(),
                },
            },
        );
        procedure.edges = procedure
            .edges
            .iter()
            .cloned()
            .map(|edge| {
                if edge.target == ret {
                    IrEdge {
                        target: id.clone(),
                        ..edge
                    }
                } else {
                    edge
                }
            })
            .collect();
        procedure.edges.insert(normal_edge(id, ret.clone()));
        procedure.nodes.get_mut(&ret).unwrap().operation = Operation::Return { value: Some(place) };
    }
}

impl ProjectProgram {
    pub fn from_metadata(
        request: &MetadataRequest,
        metadata: &ProjectMetadata,
        root_path: &str,
    ) -> Result<Self, String> {
        let mut program = Self {
            metadata: Some(metadata.clone()),
            sources: request
                .files
                .iter()
                .map(|(path, file)| (path.clone(), file.text.clone()))
                .collect(),
            next_namespace: NAMESPACE,
            ..Self::default()
        };
        program.file_namespaces.insert(root_path.into(), 0);
        let mut paths = BTreeSet::from([root_path.to_owned()]);
        loop {
            let mut next = paths.clone();
            for call in &metadata.calls {
                if paths.contains(&call.span.path)
                    && call.status == ResolutionStatus::ResolvedBody
                    && let Some(declaration) = call
                        .target
                        .as_ref()
                        .and_then(|id| metadata.declarations.iter().find(|d| &d.id == id))
                {
                    next.insert(declaration.span.path.clone());
                }
            }
            if next == paths {
                break;
            }
            paths = next;
        }
        for path in paths {
            let Some(file) = request.files.get(&path) else {
                continue;
            };
            if request
                .options
                .excluded_paths
                .iter()
                .any(|p| path == *p || path.starts_with(&format!("{p}/")))
            {
                continue;
            }
            let namespace = if let Some(value) = program.file_namespaces.get(&path) {
                *value
            } else {
                let value = program.next_namespace;
                program.next_namespace = program
                    .next_namespace
                    .checked_add(NAMESPACE)
                    .ok_or("IR file identity namespace capacity exhausted")?;
                program.file_namespaces.insert(path.clone(), value);
                value
            };
            let Ok(index) = index_bindings(request.snapshot.clone(), &path, &file.text) else {
                continue;
            };
            program.immutable_bindings.extend(
                index
                    .bindings
                    .iter()
                    .filter(|b| b.kind == super::adapters::typescript::BindingKind::Const)
                    .map(|b| remap(&b.id, namespace)),
            );
            let Ok(functions) = index_functions(&path, &file.text) else {
                continue;
            };
            for function in functions {
                let Some(declaration) = metadata
                    .declarations
                    .iter()
                    .filter(|d| {
                        d.has_body
                            && !d.declaration_only
                            && !d.excluded
                            && d.span.path == path
                            && d.implementation_span.as_ref().is_some_and(|span| {
                                span.byte_start <= function.span.byte_start
                                    && span.byte_end == function.span.byte_end
                            })
                    })
                    .min_by_key(|d| d.span.byte_end - d.span.byte_start)
                else {
                    continue;
                };
                // Captured data and non-simple parameter forms require later tasks.
                let captured = index
                    .references
                    .iter()
                    .filter(|r| {
                        r.span.byte_start >= function.span.byte_start
                            && r.span.byte_end <= function.span.byte_end
                    })
                    .any(|reference| {
                        let in_callee = metadata.calls.iter().any(|call| {
                            call.callee_span.path == path
                                && reference.span.byte_start >= call.callee_span.byte_start
                                && reference.span.byte_end <= call.callee_span.byte_end
                                && call.status == ResolutionStatus::ResolvedBody
                        });
                        !in_callee
                            && reference
                                .binding
                                .as_ref()
                                .and_then(|id| index.bindings.iter().find(|b| &b.id == id))
                                .is_some_and(|binding| {
                                    binding.declaration.byte_start < function.span.byte_start
                                        || binding.declaration.byte_end > function.span.byte_end
                                })
                    });
                let bad_parameters = index.diagnostics.iter().any(|d| {
                    d.message.contains("parameter")
                        && d.span.as_ref().is_some_and(|s| {
                            s.byte_start >= function.span.byte_start
                                && s.byte_end <= function.span.byte_end
                        })
                });
                if captured || bad_parameters {
                    continue;
                }
                let Ok((procedure, _)) =
                    lower_function_procedure(&file.text, &index, &function.span)
                else {
                    continue;
                };
                if procedure.nodes.keys().any(|id| id.local >= NAMESPACE) {
                    return Err("file-local IR identity exceeds the qualification capacity".into());
                }
                let mut procedure = remap_procedure(&procedure, namespace);
                normalize_returns(&mut procedure);
                program
                    .declarations
                    .insert(declaration.id.clone(), procedure.id.clone());
                program.procedures.insert(procedure.id.clone(), procedure);
            }
        }
        let ids: Vec<_> = program.procedures.keys().cloned().collect();
        for id in ids {
            let mut procedure = program.procedures.remove(&id).unwrap();
            program.resolve_calls(&mut procedure);
            program.procedures.insert(id, procedure);
        }
        program.classify_effects();
        Ok(program)
    }

    fn resolve_calls(&mut self, procedure: &mut ProcedureIr) {
        let Some(metadata) = &self.metadata else {
            return;
        };
        for node in procedure.nodes.values_mut() {
            let Operation::UnknownEffect {
                result: Some(result),
                definition: Some(definition),
                ..
            } = &node.operation
            else {
                continue;
            };
            if metadata.declarations.iter().any(|d| {
                self.declarations.contains_key(&d.id)
                    && d.implementation_span.as_ref() == node.span.as_ref()
            }) {
                node.operation = Operation::Literal {
                    definition: definition.clone(),
                    literal_kind: LiteralKind::FunctionReference,
                    raw: "function reference".into(),
                    cooked: None,
                    result: result.clone(),
                };
            }
        }
        let mut replacements = Vec::new();
        for node in procedure.nodes.values() {
            let Operation::UnknownEffect {
                inputs,
                result,
                affected_places,
                description,
                ..
            } = &node.operation
            else {
                continue;
            };
            if description != "unsupported TypeScript construct \"call_expression\"" {
                continue;
            }
            let Some(call) = metadata.calls.iter().find(|call| {
                node.span.as_ref() == Some(&call.span)
                    && call.status == ResolutionStatus::ResolvedBody
            }) else {
                continue;
            };
            let Some(callee) = call
                .target
                .as_ref()
                .and_then(|id| self.declarations.get(id))
            else {
                continue;
            };
            replacements.push((
                node.id.clone(),
                callee.clone(),
                inputs
                    .iter()
                    .skip(1)
                    .map(|i| i.place.clone())
                    .collect::<Vec<_>>(),
                result.clone(),
                affected_places.clone(),
                call.callee_span.clone(),
            ));
        }
        for (id, callee, mut arguments, result, affected_places, callee_span) in replacements {
            // A resolved immutable direct reference has no runtime value effect.
            // Preserve evaluation of arguments, including nested calls.
            for node in procedure.nodes.values_mut() {
                if node.span.as_ref().is_some_and(|span| {
                    span.path == callee_span.path
                        && span.byte_start >= callee_span.byte_start
                        && span.byte_end <= callee_span.byte_end
                }) {
                    node.operation = Operation::Join;
                }
            }
            let parameters = if callee == procedure.id {
                procedure.parameters.len()
            } else {
                self.procedures
                    .get(&callee)
                    .map_or(0, |p| p.parameters.len())
            };
            while arguments.len() < parameters {
                let literal = next_id(procedure);
                let place = Place::Temporary(literal.clone());
                procedure.nodes.insert(
                    literal.clone(),
                    IrNode {
                        id: literal.clone(),
                        span: procedure.nodes[&id].span.clone(),
                        operation: Operation::Literal {
                            definition: DefinitionId::new(literal.snapshot.clone(), literal.local),
                            literal_kind: LiteralKind::Undefined,
                            raw: "undefined".into(),
                            cooked: None,
                            result: place.clone(),
                        },
                    },
                );
                procedure.edges = procedure
                    .edges
                    .iter()
                    .cloned()
                    .map(|edge| {
                        if edge.target == id {
                            IrEdge {
                                target: literal.clone(),
                                ..edge
                            }
                        } else {
                            edge
                        }
                    })
                    .collect();
                procedure
                    .edges
                    .insert(normal_edge(literal.clone(), id.clone()));
                for loop_ir in &mut procedure.loops {
                    if loop_ir.members.contains(&id) {
                        loop_ir.members.insert(literal.clone());
                    }
                }
                arguments.push(place);
            }
            let call_site = CallSiteId::new(id.snapshot.clone(), id.local);
            procedure.nodes.get_mut(&id).unwrap().operation = Operation::Call {
                call_site: call_site.clone(),
                arguments,
                result,
            };
            let return_site = next_id(procedure);
            procedure.nodes.insert(
                return_site.clone(),
                IrNode {
                    id: return_site.clone(),
                    span: procedure.nodes[&id].span.clone(),
                    operation: Operation::ReturnSite { call_site },
                },
            );
            procedure.edges = procedure
                .edges
                .iter()
                .cloned()
                .map(|edge| {
                    if edge.source == id {
                        IrEdge {
                            source: return_site.clone(),
                            ..edge
                        }
                    } else {
                        edge
                    }
                })
                .collect();
            procedure.edges.insert(IrEdge {
                kind: EdgeKind::CallBypass {
                    call_site: CallSiteId::new(id.snapshot.clone(), id.local),
                },
                ..normal_edge(id.clone(), return_site.clone())
            });
            self.calls.insert(
                id,
                CallTarget {
                    callee,
                    return_site,
                    affected_places,
                },
            );
        }
        // Getter/proxy effects are outside K018. A compiler-resolved body does
        // not prove that an arbitrary property/index read is harmless.
        let local_places: BTreeSet<_> = procedure
            .parameters
            .iter()
            .map(|p| Place::Binding(p.binding.clone()))
            .chain(
                procedure
                    .nodes
                    .values()
                    .filter_map(|node| match &node.operation {
                        Operation::Write {
                            target: Place::Binding(binding),
                            ..
                        } => Some(Place::Binding(binding.clone())),
                        _ => None,
                    }),
            )
            .filter(|place| !matches!(place, Place::Binding(binding) if self.immutable_bindings.contains(binding)))
            .collect();
        for node in procedure.nodes.values_mut() {
            if let Operation::Compute {
                definition,
                inputs,
                result,
                operator: PrimitiveOperator::PropertyRead { .. } | PrimitiveOperator::IndexRead,
            } = &node.operation
            {
                node.operation = Operation::UnknownEffect {
                    effect_kind: UnknownEffectKind::Value,
                    description: "unmodeled property/index read and getter effects".into(),
                    inputs: inputs.clone(),
                    result: Some(result.clone()),
                    definition: Some(definition.clone()),
                    affected_places: local_places.clone(),
                };
            }
        }
    }

    fn classify_effects(&mut self) {
        let mut unsafe_procedures: BTreeSet<_> = self
            .procedures
            .values()
            .filter(|p| {
                p.nodes
                    .values()
                    .any(|n| matches!(n.operation, Operation::UnknownEffect { .. }))
            })
            .map(|p| p.id.clone())
            .collect();
        loop {
            let before = unsafe_procedures.len();
            for p in self.procedures.values() {
                if p.nodes
                    .keys()
                    .filter_map(|n| self.calls.get(n))
                    .any(|call| unsafe_procedures.contains(&call.callee))
                {
                    unsafe_procedures.insert(p.id.clone());
                }
            }
            if before == unsafe_procedures.len() {
                break;
            }
        }
        for target in self.calls.values_mut() {
            if !unsafe_procedures.contains(&target.callee) {
                target.affected_places.clear();
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedProcedure {
    pub procedure: ProcedureIr,
    pub diagnostics: Vec<Diagnostic>,
    pub contexts: BTreeMap<NodeId, Vec<CallSiteId>>,
    pub tabulation: Option<TabulationOutput>,
    frames: BTreeMap<CallSiteId, (NodeId, ProcedureId, String)>,
    pub sources: BTreeMap<String, String>,
}

impl PreparedProcedure {
    pub fn solve(
        &self,
        limits: &AnalysisLimits,
        control: &dyn SolverControl,
    ) -> Result<super::solver::SolverOutput, super::solver::SolverError> {
        use super::solver::{IntraproceduralSolver, Solver, SolverRequest};
        let mut remaining = limits.clone();
        if let Some(tabulation) = &self.tabulation {
            remaining.processed_path_edges = remaining
                .processed_path_edges
                .saturating_sub(tabulation.counters.processed_path_edges);
            remaining.memory_bytes = remaining
                .memory_bytes
                .saturating_sub(tabulation.counters.accounted_memory_bytes);
        }
        let mut solved = IntraproceduralSolver::new().solve(
            SolverRequest {
                procedure: &self.procedure,
                entry_facts: seed_entry_facts(&self.procedure),
                limits: remaining,
            },
            control,
        )?;
        if let Some(tabulation) = &self.tabulation {
            solved.stats.processed_path_edges += tabulation.counters.processed_path_edges;
            solved.stats.peak_memory_bytes += tabulation.counters.accounted_memory_bytes;
            if let SolverTermination::LimitExceeded(reason) = &tabulation.termination {
                solved.stats.limits_hit.insert(reason.kind.as_str().into());
                solved.termination = tabulation.termination.clone();
            }
            if matches!(tabulation.termination, SolverTermination::Cancelled(_)) {
                solved.termination = tabulation.termination.clone();
            }
            solved.diagnostics.extend(
                self.diagnostics
                    .iter()
                    .filter(|d| d.code == DiagnosticCode::AnalysisBudgetExceeded)
                    .cloned(),
            );
        }
        Ok(solved)
    }
    pub fn local(procedure: ProcedureIr, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            procedure,
            diagnostics,
            contexts: BTreeMap::new(),
            tabulation: None,
            frames: BTreeMap::new(),
            sources: BTreeMap::new(),
        }
    }

    pub fn annotate_slice(&self, slice: &mut super::slicing::SliceOutput) {
        slice.witnesses = slice
            .witnesses
            .iter()
            .cloned()
            .map(|mut witness| {
                let mut pending: Vec<_> = witness
                    .nodes
                    .iter()
                    .filter(|id| slice.graph.nodes.iter().any(|n| &n.id == *id))
                    .cloned()
                    .collect();
                let mut seen = BTreeSet::new();
                while let Some(node) = pending.pop() {
                    if !seen.insert(node.clone()) {
                        continue;
                    }
                    pending.extend(
                        slice
                            .graph
                            .edges
                            .iter()
                            .filter(|edge| edge.target == node)
                            .map(|edge| edge.source.clone()),
                    );
                    if let Some(context) = self.contexts.get(&node) {
                        witness.call_contexts.insert(context.clone());
                        for site in context {
                            if let Some((return_site, callee, label)) = self.frames.get(site) {
                                witness.summary_expansions.insert(format!(
                                    "call:{}:callee:{}:{}:return_site:{}",
                                    site.local, label, callee.local, return_site.local
                                ));
                            }
                        }
                        witness.summary_expansions.insert(format!(
                            "matched_calls:{}",
                            context
                                .iter()
                                .map(|c| c.local.to_string())
                                .collect::<Vec<_>>()
                                .join("/")
                        ));
                    }
                }
                witness
            })
            .collect();
        slice.graph.edges = slice
            .graph
            .edges
            .iter()
            .cloned()
            .map(|mut edge| {
                use super::model::RelationKind;
                if matches!(
                    self.procedure.nodes[&edge.target].operation,
                    Operation::CallBind {
                        boundary: CallBindKind::Argument,
                        ..
                    }
                ) {
                    edge.relation = RelationKind::Argument;
                } else if matches!(
                    self.procedure.nodes[&edge.source].operation,
                    Operation::CallBind {
                        boundary: CallBindKind::Return,
                        ..
                    }
                ) || matches!(
                    self.procedure.nodes[&edge.target].operation,
                    Operation::CallBind {
                        boundary: CallBindKind::Return,
                        ..
                    }
                ) {
                    edge.relation = RelationKind::Return;
                }
                edge
            })
            .collect();
        slice.graph.nodes =
            slice
                .graph
                .nodes
                .iter()
                .cloned()
                .map(|mut node| {
                    if let Some(source) = self.sources.get(&node.span.path).and_then(|s| {
                        s.get(node.span.byte_start as usize..node.span.byte_end as usize)
                    }) {
                        node.operation = format!("{} `{}`", node.operation, source.trim());
                    }
                    node
                })
                .collect();
    }

    pub fn annotate_dependency(&self, dependency: &mut super::function_results::ResultDependency) {
        dependency.call_context = self
            .contexts
            .get(&dependency.node)
            .cloned()
            .unwrap_or_default();
        for input in &mut dependency.inputs {
            self.annotate_dependency(input)
        }
    }
}

/// Expanded source spans repeat at different calls. Only compare those nodes
/// when their enclosing calls have matched return-site producers.
pub(crate) fn constrain_alignment(
    before: &PreparedProcedure,
    after: &PreparedProcedure,
    alignment: &mut super::compare::AlignmentResult,
) {
    let before_sites: BTreeMap<_, _> = before
        .frames
        .iter()
        .map(|(site, (node, _, _))| (node, site))
        .collect();
    let after_sites: BTreeMap<_, _> = after
        .frames
        .iter()
        .map(|(site, (node, _, _))| (node, site))
        .collect();
    let calls: BTreeMap<_, _> = alignment
        .nodes
        .iter()
        .filter_map(|pair| {
            let a = before_sites.get(pair.before.as_ref()?)?;
            let b = after_sites.get(pair.after.as_ref()?)?;
            (before.frames[*a].2 == after.frames[*b].2).then_some((*a, *b))
        })
        .collect();
    alignment.nodes = alignment
        .nodes
        .iter()
        .filter(|pair| {
            let a = pair.before.as_ref().and_then(|id| before.contexts.get(id));
            let b = pair.after.as_ref().and_then(|id| after.contexts.get(id));
            let matched = match (a, b) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| calls.get(a) == Some(&b))
                }
                (Some(_), None) | (None, Some(_))
                    if pair.before.is_none() || pair.after.is_none() =>
                {
                    true
                }
                _ => false,
            };
            if !matched {
                if let (Some(a), Some(b)) = (&pair.before, &pair.after) {
                    alignment.ambiguities.insert(AlignmentAmbiguity {
                        candidates: BTreeSet::from([AlignmentCandidate {
                            before: AlignmentEntity::Node(a.clone()),
                            after: AlignmentEntity::Node(b.clone()),
                        }]),
                        evidence: "call-context operation alignment is unresolved".into(),
                    });
                }
                alignment.diagnostics.insert(Diagnostic {
                    code: DiagnosticCode::AmbiguousMatch,
                    message: "call-context operation alignment is unresolved".into(),
                    snapshot: None,
                    frontier: None,
                    span: None,
                    affected_flows: BTreeSet::from(["call_context_alignment".into()]),
                });
            }
            matched
        })
        .cloned()
        .collect();
}

pub(crate) fn prepare(
    procedure: ProcedureIr,
    diagnostics: Vec<Diagnostic>,
    project: Option<&ProjectProgram>,
    limits: &AnalysisLimits,
    control: &dyn SolverControl,
) -> Result<PreparedProcedure, String> {
    let Some(project) = project else {
        return Ok(PreparedProcedure::local(procedure, diagnostics));
    };
    if procedure.nodes.keys().any(|id| id.local >= NAMESPACE) {
        return Err("entry IR identity exceeds the qualification capacity".into());
    }
    let mut program = project.clone();
    let mut procedure = procedure;
    program.resolve_calls(&mut procedure);
    program
        .procedures
        .insert(procedure.id.clone(), procedure.clone());
    program.classify_effects();
    let tabulation = tabulate(
        &program,
        &procedure.id,
        seed_entry_facts(&procedure),
        limits,
        control,
    )?;
    let mut prepared = PreparedProcedure {
        procedure,
        diagnostics,
        contexts: BTreeMap::new(),
        tabulation: Some(tabulation),
        frames: BTreeMap::new(),
        sources: program.sources.clone(),
    };
    let mut active = BTreeSet::from([prepared.procedure.id.clone()]);
    let root_calls: Vec<_> = prepared
        .procedure
        .nodes
        .keys()
        .filter(|id| program.calls.contains_key(*id))
        .cloned()
        .collect();
    for call in root_calls {
        expand_call(&mut prepared, &mut program, &call, &mut active, &[], limits)?;
    }
    // Resolution and supported direct-reference evaluation replace the old
    // lowering diagnostics. Retain every remaining unsupported operation.
    prepared.diagnostics.retain(|d| {
        d.frontier.as_ref().is_none_or(|id| {
            prepared
                .procedure
                .nodes
                .get(id)
                .is_none_or(|n| matches!(n.operation, Operation::UnknownEffect { .. }))
        })
    });
    prepared.procedure.validate().map_err(|e| e.to_string())?;
    Ok(prepared)
}

fn expand_call(
    prepared: &mut PreparedProcedure,
    program: &mut ProjectProgram,
    call_id: &NodeId,
    active: &mut BTreeSet<ProcedureId>,
    outer: &[CallSiteId],
    limits: &AnalysisLimits,
) -> Result<(), String> {
    let target = program.calls[call_id].clone();
    let call = prepared.procedure.nodes[call_id].clone();
    let Operation::Call {
        call_site,
        arguments,
        result,
    } = &call.operation
    else {
        return Ok(());
    };
    let incomplete = prepared
        .tabulation
        .as_ref()
        .is_some_and(|t| !matches!(t.termination, SolverTermination::FixedPoint));
    let recursive = active.contains(&target.callee);
    let size = program.procedures[&target.callee].nodes.len() as u64
        + program.procedures[&target.callee].parameters.len() as u64 * 2
        + 4;
    if incomplete
        || recursive
        || program.next_namespace.checked_add(NAMESPACE).is_none()
        || prepared.procedure.nodes.len() as u64 + size > limits.output_nodes
        || (prepared.procedure.nodes.len() as u64 + size).saturating_mul(1024)
            >= limits.memory_bytes
    {
        // Unknown continuation still uses the existing conservative scalar
        // transfer; its independent preserved facts may continue locally.
        prepared.procedure.edges = prepared
            .procedure
            .edges
            .iter()
            .cloned()
            .map(|edge| {
                if edge.source == *call_id && edge.target == target.return_site {
                    IrEdge {
                        kind: EdgeKind::Normal,
                        ..edge
                    }
                } else {
                    edge
                }
            })
            .collect();
        let description = if recursive {
            "recursive call evidence requires K019"
        } else {
            "call summary evidence budget exceeded"
        };
        prepared.procedure.nodes.get_mut(call_id).unwrap().operation = Operation::UnknownEffect {
            effect_kind: UnknownEffectKind::Value,
            description: description.into(),
            inputs: arguments
                .iter()
                .enumerate()
                .map(|(i, p)| ComputeInput {
                    place: p.clone(),
                    role: ComputeInputRole::Operand { index: i as u32 },
                    span: call.span.clone().unwrap(),
                })
                .collect(),
            result: result.clone(),
            definition: result
                .as_ref()
                .map(|_| DefinitionId::new(call_id.snapshot.clone(), call_id.local)),
            affected_places: target.affected_places.clone(),
        };
        prepared.diagnostics.push(Diagnostic {
            code: if recursive {
                DiagnosticCode::UnresolvedCall
            } else {
                DiagnosticCode::AnalysisBudgetExceeded
            },
            message: description.into(),
            snapshot: Some(call_id.snapshot.clone()),
            frontier: Some(call_id.clone()),
            span: call.span,
            affected_flows: BTreeSet::from(["upstream".into(), "downstream".into()]),
        });
        return Ok(());
    }
    let label = program
        .metadata
        .as_ref()
        .and_then(|metadata| {
            metadata
                .declarations
                .iter()
                .find(|d| program.declarations.get(&d.id) == Some(&target.callee))
        })
        .map(|d| format!("{}:{}", d.span.path, d.name))
        .unwrap_or_else(|| format!("procedure:{}", target.callee.local));
    prepared.frames.insert(
        call_site.clone(),
        (target.return_site.clone(), target.callee.clone(), label),
    );
    let namespace = program.next_namespace;
    program.next_namespace += NAMESPACE;
    let mut body = remap_procedure(&program.procedures[&target.callee], namespace);
    let mut context = outer.to_vec();
    context.push(call_site.clone());
    let nested: Vec<_> = program.procedures[&target.callee]
        .nodes
        .keys()
        .filter_map(|node| {
            program.calls.get(node).map(|call| {
                (
                    remap(node, namespace),
                    CallTarget {
                        callee: call.callee.clone(),
                        return_site: remap(&call.return_site, namespace),
                        affected_places: remap(&call.affected_places, namespace),
                    },
                )
            })
        })
        .collect();
    for (node, call) in &nested {
        let mut call = call.clone();
        call.affected_places.extend(target.affected_places.clone());
        program.calls.insert(node.clone(), call.clone());
    }
    let return_binding = Place::Binding(BindingId::new(
        call_id.snapshot.clone(),
        namespace + NAMESPACE - 1,
    ));
    // Return statements become bindings to a call-private carrier. Caller
    // assignments remain ordinary writes; callee parameter writes stay local.
    for node in body.nodes.values_mut() {
        if let Operation::Return { value } = &node.operation {
            node.operation = Operation::CallBind {
                call_site: call_site.clone(),
                boundary: CallBindKind::Return,
                target: return_binding.clone(),
                sources: value.iter().cloned().collect(),
                definition: DefinitionId::new(node.id.snapshot.clone(), node.id.local),
            };
        } else if matches!(node.operation, Operation::Entry | Operation::Exit) {
            node.operation = Operation::Join;
        } else if let Operation::UnknownEffect {
            affected_places, ..
        } = &mut node.operation
        {
            affected_places.extend(target.affected_places.clone());
        }
        prepared.contexts.insert(node.id.clone(), context.clone());
    }
    let return_site = target.return_site.clone();
    // Move the producer to the distinct return site, and update all caller uses.
    if let Some(old_result) = result {
        let replacement = Place::Temporary(return_site.clone());
        for node in prepared.procedure.nodes.values_mut() {
            replace_place(&mut node.operation, old_result, &replacement);
        }
        prepared
            .procedure
            .nodes
            .get_mut(&return_site)
            .unwrap()
            .operation = Operation::Read {
            source: return_binding,
            result: replacement,
        };
    }
    prepared
        .contexts
        .insert(return_site.clone(), context.clone());
    prepared.procedure.nodes.get_mut(call_id).unwrap().operation = Operation::Join;
    prepared
        .procedure
        .edges
        .retain(|e| !(e.source == *call_id && e.target == return_site));
    let mut tail = call_id.clone();
    let mut argument_nodes = BTreeSet::new();
    for (index, parameter) in body.parameters.iter().enumerate() {
        let id = next_id(&prepared.procedure);
        let source = if let Some(argument) = arguments.get(index) {
            argument.clone()
        } else {
            let place = Place::Temporary(id.clone());
            prepared.procedure.nodes.insert(
                id.clone(),
                IrNode {
                    id: id.clone(),
                    span: call.span.clone(),
                    operation: Operation::Literal {
                        definition: DefinitionId::new(id.snapshot.clone(), id.local),
                        literal_kind: LiteralKind::Undefined,
                        raw: "undefined".into(),
                        cooked: None,
                        result: place.clone(),
                    },
                },
            );
            argument_nodes.insert(id.clone());
            prepared
                .procedure
                .edges
                .insert(normal_edge(tail, id.clone()));
            tail = id;
            place
        };
        let id = next_id(&prepared.procedure);
        prepared.procedure.nodes.insert(
            id.clone(),
            IrNode {
                id: id.clone(),
                span: call.span.clone(),
                operation: Operation::CallBind {
                    call_site: call_site.clone(),
                    boundary: CallBindKind::Argument,
                    target: Place::Binding(parameter.binding.clone()),
                    sources: BTreeSet::from([source]),
                    definition: parameter.entry_definition.clone(),
                },
            },
        );
        prepared.contexts.insert(id.clone(), context.clone());
        argument_nodes.insert(id.clone());
        prepared
            .procedure
            .edges
            .insert(normal_edge(tail, id.clone()));
        tail = id;
    }
    prepared
        .procedure
        .edges
        .insert(normal_edge(tail, body.entry.clone()));
    for exit in &body.exits {
        prepared
            .procedure
            .edges
            .insert(normal_edge(exit.clone(), return_site.clone()));
    }
    // Include expanded members when a call occurs inside a caller loop.
    for loop_ir in &mut prepared.procedure.loops {
        if loop_ir.members.contains(call_id) {
            loop_ir.members.extend(body.nodes.keys().cloned());
            loop_ir.members.extend(argument_nodes.clone());
            loop_ir.members.insert(return_site.clone());
        }
    }
    prepared.procedure.nodes.extend(body.nodes);
    prepared.procedure.edges.extend(body.edges);
    prepared.procedure.loops.extend(body.loops);
    active.insert(target.callee.clone());
    for (node, _) in nested {
        expand_call(prepared, program, &node, active, &context, limits)?;
    }
    active.remove(&target.callee);
    Ok(())
}

fn replace_place(operation: &mut Operation, old: &Place, new: &Place) {
    fn one(place: &mut Place, old: &Place, new: &Place) {
        if place == old {
            *place = new.clone()
        }
    }
    match operation {
        Operation::Read { source, .. } => one(source, old, new),
        Operation::Write {
            target, sources, ..
        }
        | Operation::CallBind {
            target, sources, ..
        } => {
            one(target, old, new);
            *sources = sources
                .iter()
                .map(|p| if p == old { new.clone() } else { p.clone() })
                .collect();
        }
        Operation::Compute { inputs, .. } | Operation::UnknownEffect { inputs, .. } => {
            for input in inputs {
                one(&mut input.place, old, new)
            }
        }
        Operation::Branch { condition } => one(condition, old, new),
        Operation::Call { arguments, .. } => {
            for argument in arguments {
                one(argument, old, new)
            }
        }
        Operation::Return { value: Some(value) } => one(value, old, new),
        _ => {}
    }
}

#[cfg(test)]
mod tests;
