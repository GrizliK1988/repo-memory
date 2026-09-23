//! Construction of a selected binding's complete supported source-level slice.

use crate::ifds::branches::BranchPaths;
use crate::ifds::ir::{ComputeInputRole, IrNode, Operation, ProcedureIr};
use crate::ifds::model::{
    BindingId, Completeness, Coverage, Diagnostic, DiagnosticCode, Direction, EvidenceKind, Fact,
    FlowEdge, FlowGraph, FlowNode, NodeId, PathEnding, PathEndingKind, Place, SnapshotExtent,
    Source, SourceSpan, UnknownFrontier, Witness, WitnessId,
};
use crate::ifds::solver::{SolverOutput, SolverTermination};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

#[derive(Debug, Clone)]
pub struct SliceRequest<'a> {
    pub procedure: &'a ProcedureIr,
    pub solved: &'a SolverOutput,
    pub selected_binding: &'a BindingId,
    pub declared_scope: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceOutput {
    pub graph: FlowGraph,
    pub witnesses: BTreeSet<Witness>,
    pub path_endings: BTreeSet<PathEnding>,
    pub extent: SnapshotExtent,
    pub completeness: Completeness,
    pub diagnostics: BTreeSet<Diagnostic>,
    pub unknown_frontiers: BTreeSet<UnknownFrontier>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SliceError {
    InvalidIr(String),
    SnapshotMismatch,
    MissingSelectedBinding,
    MissingSpan(NodeId),
}

impl fmt::Display for SliceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to build flow slice: {self:?}")
    }
}

impl std::error::Error for SliceError {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ResolvedSource {
    node: NodeId,
    projection: String,
    computed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CarrierKey {
    origin: Source,
    binding: BindingId,
}

#[derive(Debug, Clone)]
struct CarrierLife {
    created_at: NodeId,
    uses: BTreeSet<NodeId>,
    killed_at: Option<NodeId>,
}

struct EndingMetadata<'a> {
    context: &'a str,
    diagnostic: Option<DiagnosticCode>,
    condition: Option<String>,
}

struct SliceBuilder<'a> {
    procedure: &'a ProcedureIr,
    facts_at: &'a BTreeMap<NodeId, BTreeSet<Fact>>,
    selected_binding: &'a BindingId,
    declared_scope: &'a str,
    diagnostics: &'a BTreeSet<Diagnostic>,
    frontiers: &'a BTreeSet<UnknownFrontier>,
    selected_origins: BTreeSet<Source>,
    relevant_origins: BTreeSet<Source>,
    definition_nodes: BTreeMap<crate::ifds::model::DefinitionId, NodeId>,
    expression_owner: BTreeMap<NodeId, NodeId>,
    graph_nodes: BTreeSet<FlowNode>,
    graph_edges: BTreeSet<FlowEdge>,
    graph_facts: BTreeSet<Fact>,
    witnesses: BTreeSet<Witness>,
    endings: BTreeSet<PathEnding>,
    relevant_diagnostics: BTreeSet<Diagnostic>,
    relevant_frontiers: BTreeSet<UnknownFrontier>,
    next_witness: u64,
    upstream_partial: bool,
    downstream_partial: bool,
    branches: Option<BranchPaths>,
}

pub fn build_flow_slice(request: SliceRequest<'_>) -> Result<SliceOutput, SliceError> {
    request
        .procedure
        .validate()
        .map_err(|error| SliceError::InvalidIr(error.to_string()))?;
    if request.selected_binding.snapshot != request.procedure.id.snapshot {
        return Err(SliceError::SnapshotMismatch);
    }
    let mut builder = SliceBuilder::new(
        request.procedure,
        &request.solved.facts_at,
        request.selected_binding,
        request.declared_scope,
        &request.solved.diagnostics,
        &request.solved.unknown_frontiers,
    )?;
    builder.build()?;
    if !matches!(request.solved.termination, SolverTermination::FixedPoint)
        && builder.relevant_frontiers.is_empty()
    {
        builder.downstream_partial = true;
    }
    Ok(builder.finish())
}

impl<'a> SliceBuilder<'a> {
    fn new(
        procedure: &'a ProcedureIr,
        facts_at: &'a BTreeMap<NodeId, BTreeSet<Fact>>,
        selected_binding: &'a BindingId,
        declared_scope: &'a str,
        diagnostics: &'a BTreeSet<Diagnostic>,
        frontiers: &'a BTreeSet<UnknownFrontier>,
    ) -> Result<Self, SliceError> {
        let definition_nodes = procedure
            .nodes
            .values()
            .filter_map(|node| {
                operation_definition(&node.operation).map(|id| (id.clone(), node.id.clone()))
            })
            .collect();
        let mut selected_origins = BTreeSet::new();
        for node in procedure.nodes.values() {
            if let Operation::Write {
                target: Place::Binding(binding),
                definition,
                ..
            } = &node.operation
                && binding == selected_binding
            {
                selected_origins.insert(Source::Write(definition.clone()));
            }
        }
        if procedure
            .parameters
            .iter()
            .any(|parameter| &parameter.binding == selected_binding)
        {
            selected_origins.insert(Source::FunctionInput(selected_binding.clone()));
        }
        if selected_origins.is_empty() {
            return Err(SliceError::MissingSelectedBinding);
        }
        let expression_owner = expression_owners(procedure);
        let branches = procedure
            .nodes
            .values()
            .any(|node| matches!(node.operation, Operation::Branch { .. }))
            .then(|| {
                BranchPaths::build(
                    procedure,
                    facts_at.get(&procedure.entry).cloned().unwrap_or_default(),
                    4096,
                )
            });
        let branch_truncated = branches.as_ref().is_some_and(|paths| paths.truncated);
        let mut builder = Self {
            procedure,
            facts_at,
            selected_binding,
            declared_scope,
            diagnostics,
            frontiers,
            selected_origins,
            relevant_origins: BTreeSet::new(),
            definition_nodes,
            expression_owner,
            graph_nodes: BTreeSet::new(),
            graph_edges: BTreeSet::new(),
            graph_facts: BTreeSet::new(),
            witnesses: BTreeSet::new(),
            endings: BTreeSet::new(),
            relevant_diagnostics: BTreeSet::new(),
            relevant_frontiers: BTreeSet::new(),
            next_witness: 1,
            upstream_partial: false,
            downstream_partial: false,
            branches,
        };
        if branch_truncated {
            builder.downstream_partial = true;
            builder.relevant_diagnostics.insert(Diagnostic {
                code: DiagnosticCode::AnalysisBudgetExceeded,
                message: "branch witness state limit (4096) exceeded".into(),
                snapshot: Some(procedure.id.snapshot.clone()),
                frontier: Some(procedure.entry.clone()),
                span: None,
                affected_flows: BTreeSet::new(),
            });
            builder.relevant_frontiers.insert(UnknownFrontier {
                node: procedure.entry.clone(),
                next_operation: "branch path validation".into(),
                diagnostic: DiagnosticCode::AnalysisBudgetExceeded,
                affected_direction: Direction::Downstream,
            });
        }
        Ok(builder)
    }

    fn build(&mut self) -> Result<(), SliceError> {
        self.relevant_origins = self.selected_origins.clone();
        self.add_selected_writes_and_upstream()?;
        self.add_downstream()?;
        self.add_controls()?;
        if self.branches.is_some() {
            self.add_branch_endings()?;
        } else {
            self.add_endings()?;
        }
        self.retain_relevant_facts();
        Ok(())
    }

    fn add_selected_writes_and_upstream(&mut self) -> Result<(), SliceError> {
        let selected: Vec<_> = self
            .procedure
            .nodes
            .values()
            .filter(|node| matches!(&node.operation,
                Operation::Write { target: Place::Binding(binding), .. } if binding == self.selected_binding))
            .cloned()
            .collect();
        for node in selected {
            if !self.reachable(&node.id) {
                continue;
            }
            self.add_node(&node, None)?;
            let Operation::Write { sources, .. } = &node.operation else {
                unreachable!()
            };
            for source in sources {
                let upstream_origins: BTreeSet<_> = self
                    .facts(&node.id)
                    .iter()
                    .filter_map(|fact| match fact {
                        Fact::Origin {
                            place: candidate,
                            source: origin,
                        } if candidate == source => Some(origin.clone()),
                        _ => None,
                    })
                    .collect();
                self.relevant_origins.extend(upstream_origins);
                let resolved = self.resolve_sources(source, &node.id, None, "value", false);
                for item in resolved {
                    self.add_resolved_node(&item.node)?;
                    let evidence = if self.is_unknown_node(&item.node) {
                        self.upstream_partial = true;
                        EvidenceKind::Unresolved {
                            diagnostic: diagnostic_for_operation(
                                &self.procedure.nodes[&item.node].operation,
                            ),
                        }
                    } else {
                        EvidenceKind::Supported
                    };
                    self.add_edge(
                        item.node,
                        node.id.clone(),
                        crate::ifds::model::RelationKind::ValueDependency,
                        Some(item.projection),
                        evidence,
                    );
                }
            }
        }
        Ok(())
    }

    fn add_downstream(&mut self) -> Result<(), SliceError> {
        let nodes: Vec<_> = self.procedure.nodes.values().cloned().collect();
        let origins: Vec<_> = self.selected_origins.iter().cloned().collect();
        for node in nodes {
            if !self.reachable(&node.id) {
                continue;
            }
            match &node.operation {
                Operation::Write { sources, .. } => {
                    for origin in &origins {
                        for source in sources {
                            if !self.place_has_origin(&node.id, source, origin) {
                                continue;
                            }
                            self.add_node(&node, None)?;
                            for item in
                                self.resolve_sources(source, &node.id, Some(origin), "value", false)
                            {
                                self.add_resolved_node(&item.node)?;
                                self.add_edge(
                                    item.node,
                                    node.id.clone(),
                                    crate::ifds::model::RelationKind::ValueDependency,
                                    Some(item.projection),
                                    EvidenceKind::Supported,
                                );
                            }
                        }
                    }
                }
                Operation::Return { value: Some(value) } => {
                    for origin in &origins {
                        if !self.place_has_origin(&node.id, value, origin) {
                            continue;
                        }
                        self.add_node(&node, None)?;
                        for item in
                            self.resolve_sources(value, &node.id, Some(origin), "value", false)
                        {
                            self.add_resolved_node(&item.node)?;
                            let relation = if item.computed {
                                crate::ifds::model::RelationKind::ValueDependency
                            } else {
                                crate::ifds::model::RelationKind::Reaches
                            };
                            self.add_edge(
                                item.node,
                                node.id.clone(),
                                relation,
                                Some(item.projection),
                                EvidenceKind::Supported,
                            );
                        }
                    }
                }
                Operation::UnknownEffect { inputs, .. } => {
                    for origin in &origins {
                        for input in inputs {
                            if !self.place_has_origin(&node.id, &input.place, origin) {
                                continue;
                            }
                            self.add_node(&node, None)?;
                            let projection = unknown_input_projection(&node.operation, &input.role);
                            for item in self.resolve_sources(
                                &input.place,
                                &node.id,
                                Some(origin),
                                &projection,
                                false,
                            ) {
                                self.add_resolved_node(&item.node)?;
                                self.add_edge(
                                    item.node,
                                    node.id.clone(),
                                    crate::ifds::model::RelationKind::Argument,
                                    Some(item.projection),
                                    EvidenceKind::Supported,
                                );
                            }
                            self.mark_frontier_relevant(&node.id);
                            self.downstream_partial = true;
                        }
                    }
                }
                Operation::Call { arguments, .. } => {
                    for origin in &origins {
                        for (index, argument) in arguments.iter().enumerate() {
                            if !self.place_has_origin(&node.id, argument, origin) {
                                continue;
                            }
                            self.add_node(&node, None)?;
                            let projection = format!("arguments[{index}]");
                            for item in self.resolve_sources(
                                argument,
                                &node.id,
                                Some(origin),
                                &projection,
                                false,
                            ) {
                                self.add_resolved_node(&item.node)?;
                                self.add_edge(
                                    item.node,
                                    node.id.clone(),
                                    crate::ifds::model::RelationKind::Argument,
                                    Some(item.projection),
                                    EvidenceKind::Supported,
                                );
                            }
                            self.mark_frontier_relevant(&node.id);
                            self.downstream_partial = true;
                        }
                    }
                }
                Operation::Compute { inputs, .. }
                    if !self.expression_owner.contains_key(&node.id) =>
                {
                    for origin in &origins {
                        for input in inputs {
                            if !self.place_has_origin(&node.id, &input.place, origin) {
                                continue;
                            }
                            self.add_node(&node, None)?;
                            let projection = role_projection(&input.role);
                            for item in self.resolve_sources(
                                &input.place,
                                &node.id,
                                Some(origin),
                                &projection,
                                false,
                            ) {
                                self.add_resolved_node(&item.node)?;
                                self.add_edge(
                                    item.node,
                                    node.id.clone(),
                                    crate::ifds::model::RelationKind::ValueDependency,
                                    Some(item.projection),
                                    EvidenceKind::Supported,
                                );
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn add_controls(&mut self) -> Result<(), SliceError> {
        if self.branches.is_none() {
            return Ok(());
        }
        let branches: Vec<_> = self
            .procedure
            .nodes
            .values()
            .filter(|node| {
                matches!(node.operation, Operation::Branch { .. }) && self.reachable(&node.id)
            })
            .cloned()
            .collect();
        for branch in branches {
            let Operation::Branch { condition } = &branch.operation else {
                unreachable!()
            };
            let guard_is_selected = self
                .selected_origins
                .iter()
                .any(|origin| self.place_has_origin(&branch.id, condition, origin));
            let controlled = controlled_nodes(self.procedure, &branch.id);
            let selected_writes: Vec<_> = controlled.iter().filter(|id| {
                matches!(&self.procedure.nodes[*id].operation,
                    Operation::Write { target: Place::Binding(binding), .. } if binding == self.selected_binding)
            }).cloned().collect();
            if !guard_is_selected && selected_writes.is_empty() {
                continue;
            }
            if self
                .branches
                .as_ref()
                .is_some_and(|paths| paths.unproven_branches.contains(&branch.id))
            {
                self.downstream_partial = true;
                self.relevant_diagnostics.insert(Diagnostic {
                    code: DiagnosticCode::UnprovenPathFeasibility,
                    message: "guard theory is outside the supported Boolean subset".into(),
                    snapshot: Some(self.procedure.id.snapshot.clone()),
                    frontier: Some(branch.id.clone()),
                    span: branch.span.clone(),
                    affected_flows: BTreeSet::new(),
                });
                self.relevant_frontiers.insert(UnknownFrontier {
                    node: branch.id.clone(),
                    next_operation: "branch outcome".into(),
                    diagnostic: DiagnosticCode::UnprovenPathFeasibility,
                    affected_direction: Direction::Downstream,
                });
            }
            self.add_node(&branch, None)?;
            if guard_is_selected {
                for origin in self.selected_origins.clone() {
                    for item in self.resolve_sources(
                        condition,
                        &branch.id,
                        Some(&origin),
                        "condition",
                        false,
                    ) {
                        self.add_resolved_node(&item.node)?;
                        self.add_edge(
                            item.node,
                            branch.id.clone(),
                            crate::ifds::model::RelationKind::ValueDependency,
                            Some(item.projection),
                            EvidenceKind::Supported,
                        );
                    }
                }
            }
            for id in controlled {
                let node = self.procedure.nodes[&id].clone();
                if !self.reachable(&id) {
                    continue;
                }
                if !matches!(node.operation, Operation::Write { .. }) {
                    continue;
                }
                if !guard_is_selected && !selected_writes.contains(&id) {
                    continue;
                }
                self.add_node(&node, None)?;
                self.add_edge(
                    branch.id.clone(),
                    id.clone(),
                    crate::ifds::model::RelationKind::Controls,
                    None,
                    EvidenceKind::Supported,
                );
                if selected_writes.contains(&id) {
                    for condition in self.edge_conditions(
                        &branch.id,
                        &id,
                        &crate::ifds::model::RelationKind::Controls,
                    ) {
                        let evidence = if self
                            .branches
                            .as_ref()
                            .is_some_and(|paths| paths.condition_unproven(&id, &condition))
                        {
                            EvidenceKind::Unresolved {
                                diagnostic: DiagnosticCode::UnprovenPathFeasibility,
                            }
                        } else {
                            EvidenceKind::Supported
                        };
                        self.graph_edges.insert(FlowEdge {
                            source: id.clone(),
                            target: id.clone(),
                            relation: crate::ifds::model::RelationKind::MayWrite,
                            projection: None,
                            condition,
                            evidence,
                        });
                    }
                }
                if guard_is_selected {
                    let Operation::Write {
                        target, definition, ..
                    } = &node.operation
                    else {
                        unreachable!()
                    };
                    let readers: Vec<_> = self
                        .procedure
                        .nodes
                        .values()
                        .filter(|candidate| {
                            matches!(&candidate.operation, Operation::Return { value: Some(_) })
                        })
                        .cloned()
                        .collect();
                    for reader in readers {
                        if let Operation::Return { value: Some(value) } = &reader.operation {
                            let conditions = self.branches.as_ref().unwrap().conditions_for(
                                &reader.id,
                                |facts| {
                                    facts.contains(&Fact::LastWrite {
                                        place: target.clone(),
                                        write: definition.clone(),
                                    }) || facts.contains(&Fact::Origin {
                                        place: value.clone(),
                                        source: Source::Write(definition.clone()),
                                    })
                                },
                            );
                            if !conditions.is_empty() {
                                self.add_node(&reader, None)?;
                                for condition in conditions {
                                    self.graph_edges.insert(FlowEdge {
                                        source: id.clone(),
                                        target: reader.id.clone(),
                                        relation: crate::ifds::model::RelationKind::Reaches,
                                        projection: Some("value".into()),
                                        condition,
                                        evidence: EvidenceKind::Supported,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn add_branch_endings(&mut self) -> Result<(), SliceError> {
        let paths = self.branches.as_ref().unwrap();
        let records: Vec<_> = self
            .procedure
            .nodes
            .values()
            .filter_map(|node| {
                let value = match &node.operation {
                    Operation::Return { value } => value.clone(),
                    _ => return None,
                };
                let states = paths.at.get(&node.id)?.clone();
                Some((node.id.clone(), value, states))
            })
            .collect();
        for (id, value, states) in records {
            for state in states {
                for origin in self.selected_origins.clone() {
                    let Some(carrier) = state.facts.iter().find_map(|fact| match fact {
                        Fact::Origin { place, source } if source == &origin => Some(place.clone()),
                        _ => None,
                    }) else {
                        continue;
                    };
                    let escaped = value.as_ref().is_some_and(|value| {
                        state.facts.contains(&Fact::Origin {
                            place: value.clone(),
                            source: origin.clone(),
                        })
                    });
                    let condition = (!state.labels.is_empty()).then(|| {
                        state
                            .labels
                            .iter()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(" && ")
                    });
                    self.add_ending_with_condition(
                        if state.unproven {
                            PathEndingKind::UnknownBoundary
                        } else if escaped {
                            PathEndingKind::ScopeBoundary
                        } else {
                            PathEndingKind::NoFurtherUse
                        },
                        origin,
                        carrier,
                        &id,
                        EndingMetadata {
                            context: if state.unproven {
                                "return path depends on an unproven guard"
                            } else if escaped {
                                "returned beyond the declared caller scope"
                            } else {
                                "no carrier escapes this return"
                            },
                            diagnostic: state
                                .unproven
                                .then_some(DiagnosticCode::UnprovenPathFeasibility),
                            condition,
                        },
                    )?;
                }
            }
        }
        let boundaries: Vec<_> = self
            .procedure
            .nodes
            .values()
            .filter_map(|node| {
                let (inputs, diagnostic) = match &node.operation {
                    Operation::UnknownEffect { inputs, .. } => (
                        inputs
                            .iter()
                            .map(|input| input.place.clone())
                            .collect::<Vec<_>>(),
                        diagnostic_for_operation(&node.operation),
                    ),
                    Operation::Call { arguments, .. } => {
                        (arguments.clone(), DiagnosticCode::UnresolvedCall)
                    }
                    _ => return None,
                };
                Some((
                    node.id.clone(),
                    inputs,
                    diagnostic,
                    self.branches
                        .as_ref()
                        .unwrap()
                        .at
                        .get(&node.id)
                        .cloned()
                        .unwrap_or_default(),
                ))
            })
            .collect();
        for (id, inputs, diagnostic, states) in boundaries {
            for state in states {
                for origin in self.selected_origins.clone() {
                    for carrier in &inputs {
                        if !state.facts.contains(&Fact::Origin {
                            place: carrier.clone(),
                            source: origin.clone(),
                        }) {
                            continue;
                        }
                        let condition = (!state.labels.is_empty()).then(|| {
                            state
                                .labels
                                .iter()
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(" && ")
                        });
                        self.add_ending_with_condition(
                            PathEndingKind::UnknownBoundary,
                            origin.clone(),
                            carrier.clone(),
                            &id,
                            EndingMetadata {
                                context: "unsupported continuation on this branch",
                                diagnostic: Some(diagnostic.clone()),
                                condition,
                            },
                        )?;
                        self.downstream_partial = true;
                    }
                }
            }
        }
        Ok(())
    }

    fn add_endings(&mut self) -> Result<(), SliceError> {
        let mut carriers = BTreeMap::<CarrierKey, CarrierLife>::new();
        for node in self.procedure.nodes.values() {
            if let Operation::Write {
                target: Place::Binding(binding),
                sources,
                definition,
            } = &node.operation
            {
                for origin in &self.selected_origins {
                    let generated = origin == &Source::Write(definition.clone());
                    let copied = sources
                        .iter()
                        .any(|source| self.place_has_origin(&node.id, source, origin));
                    if generated || copied {
                        carriers
                            .entry(CarrierKey {
                                origin: origin.clone(),
                                binding: binding.clone(),
                            })
                            .or_insert(CarrierLife {
                                created_at: node.id.clone(),
                                uses: BTreeSet::new(),
                                killed_at: None,
                            });
                    }
                }
            }
        }
        if self
            .procedure
            .parameters
            .iter()
            .any(|parameter| &parameter.binding == self.selected_binding)
        {
            let origin = Source::FunctionInput(self.selected_binding.clone());
            let first = self
                .procedure
                .nodes
                .values()
                .find(|node| {
                    matches!(&node.operation,
                    Operation::Read { source: Place::Binding(binding), .. }
                    if binding == self.selected_binding)
                })
                .map(|node| node.id.clone())
                .unwrap_or_else(|| self.procedure.entry.clone());
            carriers.insert(
                CarrierKey {
                    origin,
                    binding: self.selected_binding.clone(),
                },
                CarrierLife {
                    created_at: first,
                    uses: BTreeSet::new(),
                    killed_at: None,
                },
            );
        }

        for node in self.procedure.nodes.values() {
            if let Operation::Read {
                source: Place::Binding(binding),
                ..
            } = &node.operation
            {
                for origin in &self.selected_origins {
                    if self.place_has_origin(&node.id, &Place::Binding(binding.clone()), origin)
                        && let Some(life) = carriers.get_mut(&CarrierKey {
                            origin: origin.clone(),
                            binding: binding.clone(),
                        })
                    {
                        life.uses.insert(
                            self.expression_owner
                                .get(&node.id)
                                .cloned()
                                .unwrap_or_else(|| node.id.clone()),
                        );
                    }
                }
            }
            if let Operation::Write {
                target: Place::Binding(binding),
                sources,
                ..
            } = &node.operation
            {
                for origin in &self.selected_origins {
                    let incoming =
                        self.place_has_origin(&node.id, &Place::Binding(binding.clone()), origin);
                    let retained = sources
                        .iter()
                        .any(|source| self.place_has_origin(&node.id, source, origin));
                    if incoming
                        && !retained
                        && let Some(life) = carriers.get_mut(&CarrierKey {
                            origin: origin.clone(),
                            binding: binding.clone(),
                        })
                    {
                        life.killed_at = Some(node.id.clone());
                    }
                }
            }
        }

        let origins: Vec<_> = self.selected_origins.iter().cloned().collect();
        for node in self.procedure.nodes.values() {
            match &node.operation {
                Operation::Return { value: Some(value) } => {
                    for origin in &origins {
                        if self.place_has_origin(&node.id, value, origin) {
                            self.add_ending(
                                PathEndingKind::ScopeBoundary,
                                origin.clone(),
                                value.clone(),
                                &node.id,
                                "returned beyond the declared caller scope",
                                None,
                            )?;
                        }
                    }
                }
                Operation::UnknownEffect { inputs, .. } => {
                    for origin in &origins {
                        for input in inputs {
                            if self.place_has_origin(&node.id, &input.place, origin) {
                                self.add_ending(
                                    PathEndingKind::UnknownBoundary,
                                    origin.clone(),
                                    input.place.clone(),
                                    &node.id,
                                    "unsupported value continuation",
                                    Some(diagnostic_for_operation(&node.operation)),
                                )?;
                            }
                        }
                    }
                }
                Operation::Call { arguments, .. } => {
                    for origin in &origins {
                        for argument in arguments {
                            if self.place_has_origin(&node.id, argument, origin) {
                                self.add_ending(
                                    PathEndingKind::UnknownBoundary,
                                    origin.clone(),
                                    argument.clone(),
                                    &node.id,
                                    "unresolved call continuation",
                                    Some(DiagnosticCode::UnresolvedCall),
                                )?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let return_without_value = self
            .procedure
            .nodes
            .values()
            .find(|node| matches!(node.operation, Operation::Return { value: None }))
            .map(|node| node.id.clone());
        for (key, life) in carriers {
            let (kind, location, context) = if let Some(killed) = life.killed_at {
                (
                    PathEndingKind::NoFurtherUse,
                    killed,
                    "carrier overwritten with no remaining use",
                )
            } else if let Some(last_use) = life.uses.iter().next_back() {
                (
                    PathEndingKind::NoFurtherUse,
                    last_use.clone(),
                    "carrier has no reachable later use",
                )
            } else if let Some(exit) = &return_without_value {
                (
                    PathEndingKind::ExecutionExit,
                    exit.clone(),
                    "declared entry exits without exporting the carrier",
                )
            } else {
                (
                    PathEndingKind::NoFurtherUse,
                    life.created_at,
                    "written value is never read",
                )
            };
            self.add_ending(
                kind,
                key.origin,
                Place::Binding(key.binding),
                &location,
                context,
                None,
            )?;
        }
        Ok(())
    }

    fn finish(self) -> SliceOutput {
        let upstream = if self.upstream_partial {
            Coverage::Partial
        } else {
            Coverage::Complete
        };
        let downstream = if self.downstream_partial {
            Coverage::Partial
        } else {
            Coverage::Complete
        };
        let value_lifecycle_closed = !self.endings.is_empty()
            && self.endings.iter().all(|ending| {
                matches!(
                    ending.kind,
                    PathEndingKind::NoFurtherUse | PathEndingKind::ExecutionExit
                )
            });
        let completeness = if upstream == Coverage::Complete && downstream == Coverage::Complete {
            Completeness::CompleteForQuery
        } else {
            Completeness::Partial
        };
        let mut source_boundaries = BTreeSet::new();
        if self
            .selected_origins
            .iter()
            .any(|source| matches!(source, Source::FunctionInput(_)))
        {
            source_boundaries.insert("function_input".into());
        }
        let mut sink_boundaries = BTreeSet::new();
        if self
            .endings
            .iter()
            .any(|ending| ending.kind == PathEndingKind::ScopeBoundary)
        {
            sink_boundaries.insert("caller_scope".into());
        }
        if self
            .endings
            .iter()
            .any(|ending| ending.kind == PathEndingKind::UnknownBoundary)
        {
            sink_boundaries.insert("unknown".into());
        }
        SliceOutput {
            graph: FlowGraph {
                nodes: self.graph_nodes,
                edges: self.graph_edges,
                facts: self.graph_facts,
            },
            witnesses: self.witnesses,
            path_endings: self.endings,
            extent: SnapshotExtent {
                upstream,
                downstream,
                declared_scope: self.declared_scope.into(),
                source_boundaries,
                sink_boundaries,
                value_lifecycle_closed,
            },
            completeness,
            diagnostics: self.relevant_diagnostics,
            unknown_frontiers: self.relevant_frontiers,
        }
    }

    fn resolve_sources(
        &self,
        place: &Place,
        at: &NodeId,
        desired_origin: Option<&Source>,
        projection: &str,
        computed: bool,
    ) -> BTreeSet<ResolvedSource> {
        let mut visited = BTreeSet::new();
        self.resolve_sources_inner(
            place,
            at,
            desired_origin,
            projection,
            computed,
            &mut visited,
        )
    }

    fn resolve_sources_inner(
        &self,
        place: &Place,
        at: &NodeId,
        desired_origin: Option<&Source>,
        projection: &str,
        computed: bool,
        visited: &mut BTreeSet<(Place, NodeId)>,
    ) -> BTreeSet<ResolvedSource> {
        if !visited.insert((place.clone(), at.clone())) {
            return BTreeSet::new();
        }
        if let Some(origin) = desired_origin
            && !self.place_has_origin(at, place, origin)
        {
            return BTreeSet::new();
        }
        match place {
            Place::Binding(_) => {
                let mut resolved = BTreeSet::new();
                for definition in self.last_writes(at, place) {
                    if let Some(node) = self.definition_nodes.get(&definition) {
                        resolved.insert(ResolvedSource {
                            node: node.clone(),
                            projection: projection.into(),
                            computed,
                        });
                    }
                }
                if resolved.is_empty()
                    && self.facts(at).iter().any(|fact| {
                        matches!(fact,
                        Fact::Origin { place: candidate, source: Source::FunctionInput(_) }
                        if candidate == place)
                    })
                {
                    resolved.insert(ResolvedSource {
                        node: at.clone(),
                        projection: projection.into(),
                        computed,
                    });
                }
                resolved
            }
            Place::Temporary(producer) => {
                let Some(node) = self.procedure.nodes.get(producer) else {
                    return BTreeSet::new();
                };
                match &node.operation {
                    Operation::Read { source, .. } => self.resolve_sources_inner(
                        source,
                        producer,
                        desired_origin,
                        projection,
                        computed,
                        visited,
                    ),
                    Operation::Compute { inputs, .. } => inputs
                        .iter()
                        .filter(|input| {
                            desired_origin.is_none_or(|origin| {
                                self.place_has_origin(producer, &input.place, origin)
                            })
                        })
                        .flat_map(|input| {
                            let nested = join_projection(projection, &role_projection(&input.role));
                            self.resolve_sources_inner(
                                &input.place,
                                producer,
                                desired_origin,
                                &nested,
                                true,
                                visited,
                            )
                        })
                        .collect(),
                    Operation::Literal { .. }
                    | Operation::UnknownEffect { .. }
                    | Operation::Call { .. } => BTreeSet::from([ResolvedSource {
                        node: producer.clone(),
                        projection: projection.into(),
                        computed,
                    }]),
                    _ => BTreeSet::new(),
                }
            }
        }
    }

    fn add_node(
        &mut self,
        node: &IrNode,
        operation_override: Option<&str>,
    ) -> Result<(), SliceError> {
        let span = node
            .span
            .clone()
            .ok_or_else(|| SliceError::MissingSpan(node.id.clone()))?;
        let operation = operation_override
            .map(str::to_owned)
            .unwrap_or_else(|| operation_name(&node.operation));
        let fingerprint = operation_fingerprint(&node.operation);
        self.graph_nodes.insert(FlowNode {
            id: node.id.clone(),
            operation,
            span,
            enclosing_declaration: Some(self.declared_scope.into()),
            fingerprint,
        });
        Ok(())
    }

    fn add_resolved_node(&mut self, id: &NodeId) -> Result<(), SliceError> {
        if !self.reachable(id) {
            return Ok(());
        }
        let node = &self.procedure.nodes[id];
        let override_name =
            matches!(node.operation, Operation::Read { .. }).then_some("function_input");
        self.add_node(node, override_name)
    }

    fn add_edge(
        &mut self,
        source: NodeId,
        target: NodeId,
        relation: crate::ifds::model::RelationKind,
        projection: Option<String>,
        evidence: EvidenceKind,
    ) {
        if source == target {
            return;
        }
        let conditions = self.edge_conditions(&source, &target, &relation);
        if conditions.is_empty() {
            return;
        }
        for condition in conditions {
            let evidence = if self.branches.as_ref().is_some_and(|paths| paths.truncated) {
                EvidenceKind::Unresolved {
                    diagnostic: DiagnosticCode::AnalysisBudgetExceeded,
                }
            } else if self
                .branches
                .as_ref()
                .is_some_and(|paths| paths.condition_unproven(&target, &condition))
            {
                EvidenceKind::Unresolved {
                    diagnostic: DiagnosticCode::UnprovenPathFeasibility,
                }
            } else {
                evidence.clone()
            };
            self.graph_edges.insert(FlowEdge {
                source: source.clone(),
                target: target.clone(),
                relation: relation.clone(),
                projection: projection.clone(),
                condition,
                evidence,
            });
        }
        let path =
            shortest_path(self.procedure, &source, &target).unwrap_or_else(|| vec![source, target]);
        self.add_witness(path, evidence);
    }

    fn reachable(&self, node: &NodeId) -> bool {
        self.branches
            .as_ref()
            .is_none_or(|paths| paths.truncated || paths.at.contains_key(node))
    }

    fn edge_conditions(
        &self,
        source: &NodeId,
        target: &NodeId,
        relation: &crate::ifds::model::RelationKind,
    ) -> BTreeSet<Option<String>> {
        let Some(paths) = &self.branches else {
            return BTreeSet::from([None]);
        };
        if paths.truncated {
            return BTreeSet::from([None]);
        }
        let operation = &self.procedure.nodes[source].operation;
        match operation {
            Operation::Write { target: place, definition, .. } if *relation == crate::ifds::model::RelationKind::Reaches =>
                paths.conditions_for(target, |facts| facts.contains(&Fact::LastWrite { place: place.clone(), write: definition.clone() })),
            Operation::Write { definition, .. } | Operation::Literal { definition, .. } | Operation::Compute { definition, .. } =>
                paths.conditions_for(target, |facts| facts.iter().any(|fact| matches!(fact, Fact::Origin { source: Source::Write(candidate), .. } if candidate == definition))),
            _ => paths.conditions_for(target, |_| true),
        }
    }

    fn add_ending(
        &mut self,
        kind: PathEndingKind,
        origin: Source,
        carrier: Place,
        location_node: &NodeId,
        context: &str,
        diagnostic: Option<DiagnosticCode>,
    ) -> Result<(), SliceError> {
        self.add_ending_with_condition(
            kind,
            origin,
            carrier,
            location_node,
            EndingMetadata {
                context,
                diagnostic,
                condition: None,
            },
        )
    }

    fn add_ending_with_condition(
        &mut self,
        kind: PathEndingKind,
        origin: Source,
        carrier: Place,
        location_node: &NodeId,
        metadata: EndingMetadata<'_>,
    ) -> Result<(), SliceError> {
        let node = &self.procedure.nodes[location_node];
        let location = node
            .span
            .clone()
            .or_else(|| self.nearest_span(location_node))
            .ok_or_else(|| SliceError::MissingSpan(location_node.clone()))?;
        let start = origin_node(&origin, &self.definition_nodes)
            .unwrap_or_else(|| self.procedure.entry.clone());
        let path = shortest_path(self.procedure, &start, location_node)
            .unwrap_or_else(|| vec![start, location_node.clone()]);
        let evidence = metadata
            .diagnostic
            .clone()
            .map(|diagnostic| EvidenceKind::Unresolved { diagnostic })
            .unwrap_or(EvidenceKind::Supported);
        let witness = self.add_witness(path, evidence);
        self.endings.insert(PathEnding {
            kind,
            origin,
            carrier,
            location,
            condition: metadata.condition,
            context: metadata.context.into(),
            witness,
            model: None,
            diagnostic: metadata.diagnostic,
        });
        Ok(())
    }

    fn add_witness(&mut self, nodes: Vec<NodeId>, evidence: EvidenceKind) -> WitnessId {
        let id = WitnessId(self.next_witness);
        self.next_witness += 1;
        self.witnesses.insert(Witness {
            id,
            nodes,
            backedges: BTreeSet::new(),
            summary_expansions: BTreeSet::new(),
            evidence,
        });
        id
    }

    fn nearest_span(&self, from: &NodeId) -> Option<SourceSpan> {
        let mut current = from.clone();
        let incoming: BTreeMap<_, Vec<_>> = self
            .procedure
            .nodes
            .keys()
            .map(|node| {
                let sources = self
                    .procedure
                    .edges
                    .iter()
                    .filter(|edge| edge.target == *node)
                    .map(|edge| edge.source.clone())
                    .collect();
                (node.clone(), sources)
            })
            .collect();
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(current.clone()) {
                return None;
            }
            if let Some(span) = self.procedure.nodes[&current].span.clone() {
                return Some(span);
            }
            current = incoming.get(&current)?.first()?.clone();
        }
    }

    fn facts(&self, node: &NodeId) -> &BTreeSet<Fact> {
        static EMPTY: std::sync::LazyLock<BTreeSet<Fact>> = std::sync::LazyLock::new(BTreeSet::new);
        self.facts_at.get(node).unwrap_or(&EMPTY)
    }

    fn place_has_origin(&self, node: &NodeId, place: &Place, origin: &Source) -> bool {
        self.facts(node).contains(&Fact::Origin {
            place: place.clone(),
            source: origin.clone(),
        })
    }

    fn last_writes(
        &self,
        node: &NodeId,
        place: &Place,
    ) -> BTreeSet<crate::ifds::model::DefinitionId> {
        self.facts(node)
            .iter()
            .filter_map(|fact| match fact {
                Fact::LastWrite {
                    place: candidate,
                    write,
                } if candidate == place => Some(write.clone()),
                _ => None,
            })
            .collect()
    }

    fn is_unknown_node(&self, node: &NodeId) -> bool {
        matches!(
            self.procedure.nodes[node].operation,
            Operation::UnknownEffect { .. } | Operation::Call { .. }
        )
    }

    fn mark_frontier_relevant(&mut self, node: &NodeId) {
        for frontier in self
            .frontiers
            .iter()
            .filter(|frontier| &frontier.node == node)
        {
            self.relevant_frontiers.insert(frontier.clone());
        }
        for diagnostic in self
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.frontier.as_ref() == Some(node))
        {
            self.relevant_diagnostics.insert(diagnostic.clone());
        }
    }

    fn retain_relevant_facts(&mut self) {
        let visible_definitions: BTreeSet<_> = self
            .graph_nodes
            .iter()
            .filter_map(|node| operation_definition(&self.procedure.nodes[&node.id].operation))
            .cloned()
            .collect();
        self.relevant_origins
            .extend(visible_definitions.iter().cloned().map(Source::Write));
        let guarded_facts: Vec<&BTreeSet<Fact>> = self
            .branches
            .as_ref()
            .filter(|paths| !paths.truncated)
            .map(|paths| {
                paths
                    .at
                    .values()
                    .flat_map(|states| states.iter().map(|state| &state.facts))
                    .collect()
            })
            .unwrap_or_else(|| self.facts_at.values().collect());
        for facts in guarded_facts {
            self.graph_facts.extend(
                facts
                    .iter()
                    .filter(|fact| match fact {
                        Fact::Origin {
                            source: Source::Write(definition),
                            ..
                        } => visible_definitions.contains(definition),
                        Fact::Origin { source, .. } => self.relevant_origins.contains(source),
                        Fact::LastWrite { write, .. } => visible_definitions.contains(write),
                        Fact::Zero => false,
                    })
                    .cloned(),
            );
        }
    }
}

fn operation_definition(operation: &Operation) -> Option<&crate::ifds::model::DefinitionId> {
    match operation {
        Operation::Write { definition, .. }
        | Operation::Literal { definition, .. }
        | Operation::Compute { definition, .. } => Some(definition),
        Operation::UnknownEffect { definition, .. } => definition.as_ref(),
        _ => None,
    }
}

fn expression_owners(procedure: &ProcedureIr) -> BTreeMap<NodeId, NodeId> {
    let mut owners = BTreeMap::new();
    for node in procedure.nodes.values() {
        let roots: Vec<_> = match &node.operation {
            Operation::Write { sources, .. } => sources.iter().cloned().collect(),
            Operation::Return { value } => value.iter().cloned().collect(),
            _ => Vec::new(),
        };
        for root in roots {
            mark_expression_owner(
                procedure,
                &root,
                &node.id,
                &mut owners,
                &mut BTreeSet::new(),
            );
        }
    }
    owners
}

fn mark_expression_owner(
    procedure: &ProcedureIr,
    place: &Place,
    owner: &NodeId,
    owners: &mut BTreeMap<NodeId, NodeId>,
    visited: &mut BTreeSet<NodeId>,
) {
    let Place::Temporary(producer) = place else {
        return;
    };
    if !visited.insert(producer.clone()) {
        return;
    }
    let Some(node) = procedure.nodes.get(producer) else {
        return;
    };
    match &node.operation {
        Operation::Read { .. } => {
            owners.insert(producer.clone(), owner.clone());
        }
        Operation::Compute { inputs, .. } => {
            owners.insert(producer.clone(), owner.clone());
            for input in inputs {
                mark_expression_owner(procedure, &input.place, owner, owners, visited);
            }
        }
        _ => {}
    }
}

fn origin_node(
    origin: &Source,
    definitions: &BTreeMap<crate::ifds::model::DefinitionId, NodeId>,
) -> Option<NodeId> {
    match origin {
        Source::Write(definition) | Source::ParameterEntry(definition) => {
            definitions.get(definition).cloned()
        }
        Source::FunctionInput(_) | Source::ModeledExternal { .. } => None,
    }
}

fn operation_name(operation: &Operation) -> String {
    match operation {
        Operation::Entry => "entry",
        Operation::Read { .. } => "read",
        Operation::Write { .. } => "write",
        Operation::Literal { .. } => "literal",
        Operation::Compute { .. } => "compute",
        Operation::Branch { .. } => "branch",
        Operation::Join => "join",
        Operation::Call { .. } => "call",
        Operation::ReturnSite { .. } => "return_site",
        Operation::Return { .. } => "return",
        Operation::Exit => "exit",
        Operation::UnknownEffect { .. } => "unknown_effect",
    }
    .into()
}

fn operation_fingerprint(operation: &Operation) -> String {
    match operation {
        Operation::Write { .. } => "write".into(),
        Operation::Literal {
            literal_kind,
            raw,
            cooked,
            ..
        } => format!("literal:{literal_kind:?}:{raw}:{cooked:?}"),
        Operation::Compute {
            operator, inputs, ..
        } => {
            let roles: Vec<_> = inputs.iter().map(|input| &input.role).collect();
            format!("compute:{operator:?}:{roles:?}")
        }
        Operation::UnknownEffect {
            effect_kind,
            description,
            ..
        } => format!("unknown:{effect_kind:?}:{description}"),
        other => operation_name(other),
    }
}

fn role_projection(role: &ComputeInputRole) -> String {
    match role {
        ComputeInputRole::Operand { index } => format!("operands[{index}]"),
        ComputeInputRole::TemplateChunk { index } => format!("template.chunks[{index}]"),
        ComputeInputRole::TemplateInterpolation { index } => {
            format!("template.interpolations[{index}]")
        }
    }
}

fn unknown_input_projection(operation: &Operation, role: &ComputeInputRole) -> String {
    let Operation::UnknownEffect { description, .. } = operation else {
        return role_projection(role);
    };
    match role {
        ComputeInputRole::Operand { index } if description.contains("call_expression") => {
            if *index == 0 {
                "callee".into()
            } else {
                format!("arguments[{}]", index - 1)
            }
        }
        _ => role_projection(role),
    }
}

fn join_projection(outer: &str, inner: &str) -> String {
    if outer.is_empty() {
        inner.into()
    } else {
        format!("{outer}.{inner}")
    }
}

fn diagnostic_for_operation(operation: &Operation) -> DiagnosticCode {
    match operation {
        Operation::Call { .. } => DiagnosticCode::UnresolvedCall,
        Operation::UnknownEffect { .. } => DiagnosticCode::UnsupportedSyntax,
        _ => DiagnosticCode::UnknownExternalEffect,
    }
}

fn controlled_nodes(procedure: &ProcedureIr, branch: &NodeId) -> BTreeSet<NodeId> {
    fn reachable_from(procedure: &ProcedureIr, start: &NodeId) -> BTreeSet<NodeId> {
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([start.clone()]);
        while let Some(node) = queue.pop_front() {
            if !seen.insert(node.clone()) {
                continue;
            }
            for edge in procedure.edges.iter().filter(|edge| edge.source == node) {
                queue.push_back(edge.target.clone());
            }
        }
        seen
    }
    let arms: Vec<_> = procedure
        .edges
        .iter()
        .filter(|edge| {
            &edge.source == branch && matches!(edge.kind, crate::ifds::ir::EdgeKind::Branch { .. })
        })
        .map(|edge| reachable_from(procedure, &edge.target))
        .collect();
    if arms.len() != 2 {
        return BTreeSet::new();
    }
    arms[0].symmetric_difference(&arms[1]).cloned().collect()
}

fn shortest_path(procedure: &ProcedureIr, source: &NodeId, target: &NodeId) -> Option<Vec<NodeId>> {
    if source == target {
        return Some(vec![source.clone()]);
    }
    let mut queue = VecDeque::from([source.clone()]);
    let mut previous = BTreeMap::<NodeId, NodeId>::new();
    let mut seen = BTreeSet::from([source.clone()]);
    while let Some(node) = queue.pop_front() {
        for edge in procedure.edges.iter().filter(|edge| edge.source == node) {
            if seen.insert(edge.target.clone()) {
                previous.insert(edge.target.clone(), node.clone());
                if &edge.target == target {
                    let mut path = vec![target.clone()];
                    let mut current = target.clone();
                    while &current != source {
                        current = previous.get(&current)?.clone();
                        path.push(current.clone());
                    }
                    path.reverse();
                    return Some(path);
                }
                queue.push_back(edge.target.clone());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ifds::adapters::typescript::{index_bindings, lower_containing_procedure};
    use crate::ifds::fixtures::evaluate_reference;
    use crate::ifds::model::{
        AnalysisLimits, AnalysisStats, BindingId, RelationKind, SnapshotId, SnapshotSide,
    };
    use crate::ifds::provenance::seed_entry_facts;
    use crate::ifds::solver::{IntraproceduralSolver, NoopSolverControl, Solver, SolverRequest};

    fn snapshot() -> SnapshotId {
        SnapshotId {
            side: SnapshotSide::Before,
            revision: "k010".into(),
            content_id: "k010-content".into(),
        }
    }

    fn limits() -> AnalysisLimits {
        AnalysisLimits {
            time_ms: 10_000,
            memory_bytes: u64::MAX,
            processed_path_edges: 100_000,
            witnesses_per_relation: 10,
            output_nodes: 10_000,
        }
    }

    fn analyze(source: &str, selected: &str) -> (ProcedureIr, BindingId, SolverOutput) {
        let index = index_bindings(snapshot(), "main.ts", source).unwrap();
        let binding = index
            .bindings
            .iter()
            .find(|binding| binding.name == selected)
            .unwrap();
        let lowered = lower_containing_procedure(source, &index, &binding.id).unwrap();
        let solved = IntraproceduralSolver::new()
            .solve(
                SolverRequest {
                    procedure: &lowered.procedure,
                    entry_facts: seed_entry_facts(&lowered.procedure),
                    limits: limits(),
                },
                &NoopSolverControl,
            )
            .unwrap();
        (lowered.procedure, binding.id.clone(), solved)
    }

    fn slice(source: &str, selected: &str) -> SliceOutput {
        let (procedure, binding, solved) = analyze(source, selected);
        build_flow_slice(SliceRequest {
            procedure: &procedure,
            solved: &solved,
            selected_binding: &binding,
            declared_scope: "function test",
        })
        .unwrap()
    }

    fn node_with_text<'a>(output: &'a SliceOutput, source: &str, needle: &str) -> &'a FlowNode {
        output
            .graph
            .nodes
            .iter()
            .find(|node| {
                &source[node.span.byte_start as usize..node.span.byte_end as usize] == needle
            })
            .unwrap_or_else(|| panic!("missing graph node for {needle:?}"))
    }

    #[test]
    fn ifds_k010_complete_copy_chain() {
        let source = "function test() { let x = 1; const copy = x; x = 0; const result = copy + 1; return result; }";
        let output = slice(source, "x");
        let a = node_with_text(&output, source, "x = 1");
        let b = node_with_text(&output, source, "copy = x");
        let c = node_with_text(&output, source, "x = 0");
        let d = node_with_text(&output, source, "result = copy + 1");
        let r = node_with_text(&output, source, "return result;");
        for (from, to) in [(a, b), (b, d)] {
            assert!(output.graph.edges.iter().any(|edge| {
                edge.source == from.id
                    && edge.target == to.id
                    && edge.relation == RelationKind::ValueDependency
            }));
        }
        assert!(output.graph.edges.iter().any(|edge| {
            edge.source == a.id
                && edge.target == b.id
                && edge.projection.as_deref() == Some("value")
        }));
        assert!(output.graph.edges.iter().any(|edge| {
            edge.source == b.id
                && edge.target == d.id
                && edge.projection.as_deref() == Some("value.operands[0]")
        }));
        assert!(output.graph.edges.iter().any(|edge| {
            edge.source == d.id
                && edge.target == r.id
                && edge.relation == RelationKind::Reaches
                && edge.projection.as_deref() == Some("value")
        }));
        assert!(
            !output.graph.edges.iter().any(|edge| {
                edge.source == c.id && (edge.target == d.id || edge.target == r.id)
            })
        );
        assert!(output.path_endings.iter().any(|ending| {
            ending.kind == PathEndingKind::ScopeBoundary && ending.location == r.span
        }));
        assert!(!output.extent.value_lifecycle_closed);
    }

    #[test]
    fn ifds_k010_one_copy_survives() {
        let source = "function test() { let x = 1; const y = x; x = 2; return y; }";
        let output = slice(source, "x");
        let overwrite = node_with_text(&output, source, "x = 2");
        let returned = node_with_text(&output, source, "return y;");
        assert!(output.path_endings.iter().any(|ending| {
            ending.kind == PathEndingKind::NoFurtherUse
                && ending.location == overwrite.span
                && matches!(ending.carrier, Place::Binding(_))
        }));
        assert!(output.path_endings.iter().any(|ending| {
            ending.kind == PathEndingKind::ScopeBoundary && ending.location == returned.span
        }));
        assert!(!output.extent.value_lifecycle_closed);
    }

    #[test]
    fn ifds_k010_last_carrier_ends() {
        let overwritten = slice(
            "function test() { let x = 1; const y = x; x = 2; y = 3; }",
            "x",
        );
        assert!(overwritten.path_endings.iter().any(|ending| {
            ending.kind == PathEndingKind::NoFurtherUse && ending.context.contains("overwritten")
        }));

        let last_use_source = "function test() { let x = 1; const y = x + 1; }";
        let last_use = slice(last_use_source, "x");
        let consumer = node_with_text(&last_use, last_use_source, "y = x + 1");
        assert!(last_use.path_endings.iter().any(|ending| {
            ending.kind == PathEndingKind::NoFurtherUse && ending.location == consumer.span
        }));

        let unused_source = "function test() { let x = 1; const y = 2; }";
        let unused = slice(unused_source, "x");
        let write = node_with_text(&unused, unused_source, "x = 1");
        assert!(unused.path_endings.iter().any(|ending| {
            ending.kind == PathEndingKind::NoFurtherUse && ending.location == write.span
        }));
        assert!(unused.extent.value_lifecycle_closed);
    }

    #[test]
    fn ifds_k010_returned_value_escapes() {
        let escaped_source = "function test() { let x = 1; return x; }";
        let escaped = slice(escaped_source, "x");
        assert!(
            escaped
                .path_endings
                .iter()
                .any(|ending| ending.kind == PathEndingKind::ScopeBoundary)
        );
        assert!(!escaped.extent.value_lifecycle_closed);

        let exited = slice("function test() { let x = 1; return; }", "x");
        assert!(
            exited
                .path_endings
                .iter()
                .any(|ending| ending.kind == PathEndingKind::ExecutionExit)
        );
        assert!(exited.extent.value_lifecycle_closed);
    }

    #[test]
    fn ifds_k010_no_sibling_expansion() {
        let source = "function test(input: number, sibling: number) { let x = input; const y = x + sibling; const unrelated = sibling + 1; return y; }";
        let output = slice(source, "x");
        assert!(output.graph.nodes.iter().any(|node| {
            &source[node.span.byte_start as usize..node.span.byte_end as usize] == "y = x + sibling"
        }));
        assert!(!output.graph.nodes.iter().any(|node| {
            &source[node.span.byte_start as usize..node.span.byte_end as usize]
                == "unrelated = sibling + 1"
        }));
    }

    #[test]
    fn ifds_k010_unknown_is_open() {
        let source = "function test(input: number) { const x = input; mystery(x); return x; }";
        let output = slice(source, "x");
        let unknown = node_with_text(&output, source, "mystery(x)");
        let returned = node_with_text(&output, source, "return x;");
        assert!(output.graph.edges.iter().any(|edge| {
            edge.target == unknown.id
                && edge.relation == RelationKind::Argument
                && edge.projection.as_deref() == Some("arguments[0]")
        }));
        assert!(
            output
                .graph
                .edges
                .iter()
                .any(|edge| edge.target == returned.id)
        );
        assert!(
            output
                .path_endings
                .iter()
                .any(|ending| ending.kind == PathEndingKind::UnknownBoundary)
        );
        assert_eq!(output.extent.downstream, Coverage::Partial);
        assert!(!output.extent.value_lifecycle_closed);
    }

    #[test]
    fn ifds_k010_sliced_equals_unsliced() {
        for source in [
            "function test(input: number) { let x = input + 1; const y = x * 2; return y; }",
            "function test() { let x = 1; x = 2; }",
        ] {
            let (procedure, binding, solved) = analyze(source, "x");
            let production = build_flow_slice(SliceRequest {
                procedure: &procedure,
                solved: &solved,
                selected_binding: &binding,
                declared_scope: "function test",
            })
            .unwrap();
            let reference =
                evaluate_reference(&procedure, &[seed_entry_facts(&procedure)], 100_000).unwrap();
            assert!(reference.complete);
            let reference_solver = SolverOutput {
                facts_at: reference.facts_at,
                predecessors: BTreeSet::new(),
                evidence: BTreeSet::new(),
                unsupported: BTreeSet::new(),
                termination: SolverTermination::FixedPoint,
                diagnostics: BTreeSet::new(),
                unknown_frontiers: BTreeSet::new(),
                region_completeness: BTreeMap::new(),
                stats: AnalysisStats {
                    elapsed_ms: 0,
                    peak_memory_bytes: 0,
                    processed_path_edges: reference.processed_states,
                    graph_nodes: 0,
                    graph_edges: 0,
                    witnesses_emitted: 0,
                    witnesses_omitted: 0,
                    limits_hit: BTreeSet::new(),
                    graph_truncated: false,
                    witnesses_truncated: false,
                },
            };
            let full_reference = build_flow_slice(SliceRequest {
                procedure: &procedure,
                solved: &reference_solver,
                selected_binding: &binding,
                declared_scope: "function test",
            })
            .unwrap();
            assert_eq!(production.graph, full_reference.graph);
            assert_eq!(production.path_endings, full_reference.path_endings);
            assert_eq!(production.extent, full_reference.extent);
            assert_eq!(production.completeness, full_reference.completeness);
        }
    }
}
