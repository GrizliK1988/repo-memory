//! Human presentation only. Structured grouping, evidence, and identity stay intact.

use super::*;

type ObservationKey = (LogicalNodeId, String);
type Clauses = BTreeSet<GuardClause>;
type FlowConditions = (Option<Clauses>, Option<Clauses>, bool, bool, ReachingRules);
type ReachingRules = Vec<(
    LogicalNodeId,
    Option<Option<Clauses>>,
    Option<Option<Clauses>>,
)>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Change {
    Expression(BTreeSet<LogicalNodeId>),
    Sources(
        BTreeSet<LogicalNodeId>,
        BTreeSet<LogicalNodeId>,
        String,
        String,
    ),
    Guards(BTreeSet<LogicalNodeId>),
    ControlGuards(BTreeSet<LogicalNodeId>),
    Selection(ObservationKey),
    Observation(FindingKind, LogicalNodeId),
}

struct Text<'a> {
    report: &'a VariableSourceReport,
    full: &'a VariableFlowReport,
    sources: BTreeMap<LogicalNodeId, &'a SourceDefinition>,
    controls: BTreeMap<LogicalNodeId, &'a ControlDefinition>,
    observations: BTreeMap<ObservationKey, &'a Observation>,
}

pub(super) fn render(
    report: &VariableSourceReport,
    full: &VariableFlowReport,
    verbose: bool,
) -> String {
    if let Err(error) = report.validate_with_full(full) {
        return format!("Evidence mismatch: {error}; no flow claims rendered.");
    }
    let visible: BTreeSet<_> = report
        .observation_groups
        .iter()
        .flat_map(|group| &group.members)
        .map(|member| (member.id, member.projection.clone()))
        .collect();
    let text = Text {
        report,
        full,
        sources: report
            .sources
            .iter()
            .map(|source| (source.id, source))
            .collect(),
        controls: report
            .controls
            .iter()
            .map(|control| (control.id, control))
            .collect(),
        observations: report
            .observations
            .iter()
            .filter(|observation| {
                visible.contains(&(observation.id, observation.projection.clone()))
            })
            .map(|observation| {
                (
                    (observation.id, observation.projection.clone()),
                    observation,
                )
            })
            .collect(),
    };
    let mut lines = if verbose {
        // The legacy footer is replaced by notices with independent coverage.
        let legacy = report.render_legacy_text();
        let mut lines: Vec<_> = legacy.lines().map(str::to_owned).collect();
        lines.pop();
        if legacy.starts_with("No established source or logic change")
            && text
                .groups()
                .keys()
                .any(|change| matches!(change, Change::Observation(_, _)))
        {
            // The compatibility summary predates observations inferred from
            // explicit node presence. Do not repeat its empty-change sentence.
            lines = text.short();
        }
        text.details(&mut lines);
        lines
    } else {
        text.short()
    };
    text.notices(&mut lines);
    lines.join("\n")
}

fn always(clauses: Option<&Clauses>) -> bool {
    clauses.is_some_and(|clauses| {
        clauses.len() == 1 && clauses.iter().next().unwrap().terms.is_empty()
    })
}

fn location(site: &SourceSite) -> String {
    format!("{}:{}", site.span.path, site.span.start_line)
}

fn operation(site: &SourceSite) -> &str {
    site.operation
        .strip_prefix("write `")
        .or_else(|| site.operation.strip_prefix("return `"))
        .and_then(|code| code.strip_suffix('`'))
        .unwrap_or(&site.operation)
}

fn input_role(projection: &str) -> String {
    if let Some(index) = projection
        .strip_prefix("value.operands[")
        .and_then(|value| value.strip_suffix(']'))
        && !index.is_empty()
        && index.chars().all(|ch| ch.is_ascii_digit())
    {
        format!("operand {index}")
    } else {
        projection.into()
    }
}

fn source_site(source: &SourceDefinition, side: SnapshotSide) -> Option<&SourceSite> {
    match side {
        SnapshotSide::Before => source.before.as_ref(),
        SnapshotSide::After => source.after.as_ref(),
    }
}

fn upstream(source: &SourceDefinition, side: SnapshotSide) -> &BTreeSet<LogicalNodeId> {
    match side {
        SnapshotSide::Before => &source.before_upstream,
        SnapshotSide::After => &source.after_upstream,
    }
}

fn assignment_guard(source: &SourceDefinition, side: SnapshotSide) -> Option<&Clauses> {
    match side {
        SnapshotSide::Before => source.before_assignment_guard.as_ref(),
        SnapshotSide::After => source.after_assignment_guard.as_ref(),
    }
}

impl Text<'_> {
    fn changed_expression(&self, id: LogicalNodeId) -> bool {
        let (before, after) = self.sites(id);
        before
            .zip(after)
            .is_some_and(|(before, after)| before.operation != after.operation)
    }

    fn sites(&self, id: LogicalNodeId) -> (Option<&SourceSite>, Option<&SourceSite>) {
        if let Some(control) = self.controls.get(&id) {
            (control.before.as_ref(), control.after.as_ref())
        } else if let Some(source) = self.sources.get(&id) {
            (source.before.as_ref(), source.after.as_ref())
        } else {
            (None, None)
        }
    }

    fn roots(&self, changes: &BTreeSet<LogicalNodeId>) -> BTreeSet<LogicalNodeId> {
        changes
            .iter()
            .map(|id| {
                if self.changed_expression(*id)
                    && (self.controls.contains_key(id)
                        || self.sources.get(id).is_some_and(|source| {
                            matches!(source.role, SourceRole::Writer | SourceRole::Input)
                        }))
                {
                    return *id;
                }
                // An added/changed operand belongs to its changed immediate producer,
                // only when that producer is unique. Never pick an arbitrary origin.
                let parents: Vec<_> = self
                    .sources
                    .values()
                    .filter(|source| {
                        self.changed_expression(source.id)
                            && (source.before_upstream.contains(id)
                                || source.after_upstream.contains(id))
                    })
                    .map(|source| source.id)
                    .collect();
                if parents.len() == 1 { parents[0] } else { *id }
            })
            .collect()
    }

    fn affected<'a>(&'a self, finding: &Finding) -> Vec<&'a Observation> {
        let id = finding.observation.unwrap_or(finding.subject);
        self.observations
            .values()
            .copied()
            .filter(|observation| {
                observation.id == id
                    && finding
                        .projection
                        .as_ref()
                        .is_none_or(|projection| *projection == observation.projection)
            })
            .collect()
    }

    fn guard_causes(&self, source: &SourceDefinition) -> BTreeSet<LogicalNodeId> {
        let controls: BTreeSet<_> = source
            .before_assignment_guard
            .iter()
            .chain(source.after_assignment_guard.iter())
            .flat_map(|clauses| clauses.iter())
            .flat_map(|clause| &clause.terms)
            .map(|term| term.control)
            .collect();
        controls
            .into_iter()
            .filter(|id| {
                let outcomes = |clauses: Option<&Clauses>| {
                    clauses
                        .into_iter()
                        .flat_map(|clauses| clauses.iter())
                        .flat_map(|clause| &clause.terms)
                        .filter(|term| term.control == *id)
                        .map(|term| term.outcome)
                        .collect::<BTreeSet<_>>()
                };
                outcomes(source.before_assignment_guard.as_ref())
                    != outcomes(source.after_assignment_guard.as_ref())
                    || self.changed_expression(*id)
            })
            .collect()
    }

    fn guard_writers(
        &self,
        causes: &BTreeSet<LogicalNodeId>,
        members: &BTreeSet<ObservationKey>,
    ) -> BTreeSet<LogicalNodeId> {
        members
            .iter()
            .filter_map(|key| self.observations.get(key))
            .flat_map(|observation| {
                observation
                    .before_state
                    .iter()
                    .chain(observation.after_state.iter())
                    .flat_map(|state| state.sources.iter().copied())
                    .chain(std::iter::once(observation.id))
            })
            .filter(|id| {
                self.sources.get(id).is_some_and(|source| {
                    source.before_assignment_guard != source.after_assignment_guard
                        && self.guard_causes(source) == *causes
                })
            })
            .collect()
    }

    fn groups(&self) -> BTreeMap<Change, BTreeSet<ObservationKey>> {
        let mut groups: BTreeMap<Change, BTreeSet<ObservationKey>> = BTreeMap::new();
        for finding in &self.report.findings {
            if finding.observation.is_none()
                && !matches!(
                    finding.kind,
                    FindingKind::ExpressionChanged
                        | FindingKind::ObservationAdded
                        | FindingKind::ObservationRemoved
                        | FindingKind::WriteAdded
                        | FindingKind::WriteRemoved
                )
            {
                continue;
            }
            for observation in self.affected(finding) {
                let reference = (observation.id, observation.projection.clone());
                let change = match finding.kind {
                    FindingKind::ExpressionChanged => {
                        let changes = if finding.changed_operations.is_empty() {
                            BTreeSet::from([finding.subject])
                        } else {
                            finding.changed_operations.clone()
                        };
                        for root in self.roots(&changes) {
                            groups
                                .entry(Change::Expression(BTreeSet::from([root])))
                                .or_default()
                                .insert(reference.clone());
                        }
                        continue;
                    }
                    FindingKind::SourceSetChanged if self.changed_expression(observation.id) => {
                        Change::Expression(BTreeSet::from([observation.id]))
                    }
                    FindingKind::SourceSetChanged => Change::Sources(
                        finding.before_sources.clone(),
                        finding.after_sources.clone(),
                        observation_state_key(&observation.before_state),
                        observation_state_key(&observation.after_state),
                    ),
                    FindingKind::SelectionChanged => {
                        // A changed source set already prints its exact reaching rules.
                        // Do not explain the same source/selection transition twice.
                        if self.report.findings.iter().any(|other| {
                            other.kind == FindingKind::SourceSetChanged
                                && other.observation == Some(observation.id)
                                && other
                                    .projection
                                    .as_ref()
                                    .is_none_or(|projection| *projection == observation.projection)
                        }) {
                            continue;
                        }
                        let changed_guards: BTreeSet<_> = observation
                            .before_state
                            .iter()
                            .chain(observation.after_state.iter())
                            .flat_map(|state| &state.sources)
                            .copied()
                            .chain(std::iter::once(observation.id))
                            .filter(|id| {
                                self.sources.get(id).is_some_and(|source| {
                                    source.before.is_some()
                                        && source.after.is_some()
                                        && source.before_assignment_guard
                                            != source.after_assignment_guard
                                })
                            })
                            .collect();
                        if changed_guards.is_empty() {
                            Change::Selection(reference.clone())
                        } else {
                            for id in changed_guards {
                                let causes = self.guard_causes(self.sources[&id]);
                                let change = if causes.is_empty() {
                                    Change::Guards(BTreeSet::from([id]))
                                } else {
                                    Change::ControlGuards(causes)
                                };
                                groups.entry(change).or_default().insert(reference.clone());
                            }
                            continue;
                        }
                    }
                    FindingKind::ObservationAdded
                    | FindingKind::ObservationRemoved
                    | FindingKind::ObservationFlowAdded
                    | FindingKind::ObservationFlowRemoved => {
                        Change::Observation(finding.kind.clone(), observation.id)
                    }
                    FindingKind::WriteAdded | FindingKind::WriteRemoved
                        if !self.unused_write(finding.subject) =>
                    {
                        Change::Observation(
                            if finding.kind == FindingKind::WriteAdded {
                                FindingKind::ObservationAdded
                            } else {
                                FindingKind::ObservationRemoved
                            },
                            observation.id,
                        )
                    }
                    _ => continue,
                };
                groups.entry(change).or_default().insert(reference);
            }
        }
        // A newly created/removed consumer can have node/edge evidence without
        // a compact Finding. Use recorded presence and input facts, not syntax.
        for (reference, observation) in &self.observations {
            if !self
                .sources
                .get(&observation.id)
                .is_some_and(|source| source.role == SourceRole::Writer)
                || self.unused_write(observation.id)
            {
                continue;
            }
            let Some(alignment) = self
                .full
                .alignment
                .iter()
                .find(|alignment| alignment.logical == observation.id)
            else {
                continue;
            };
            let kind = match (&alignment.before, &alignment.after) {
                (None, Some(_))
                    if observation
                        .after_state
                        .as_ref()
                        .is_some_and(|state| !state.sources.is_empty()) =>
                {
                    FindingKind::ObservationAdded
                }
                (Some(_), None)
                    if observation
                        .before_state
                        .as_ref()
                        .is_some_and(|state| !state.sources.is_empty()) =>
                {
                    FindingKind::ObservationRemoved
                }
                _ => continue,
            };
            groups
                .entry(Change::Observation(kind, observation.id))
                .or_default()
                .insert(reference.clone());
        }
        groups
    }

    fn clauses(&self, clauses: Option<&Clauses>, side: SnapshotSide) -> String {
        let Some(clauses) = clauses else {
            return "unresolved".into();
        };
        if clauses.is_empty() {
            return "never".into();
        }
        clauses
            .iter()
            .map(|clause| {
                if clause.terms.is_empty() {
                    return "always".into();
                }
                let expression = clause
                    .terms
                    .iter()
                    .map(|term| {
                        let mut expression = render_clauses(
                            &BTreeSet::from([GuardClause {
                                terms: BTreeSet::from([term.clone()]),
                            }]),
                            &self.controls,
                            side,
                        );
                        // Equal display text is not evidence of equal guard value versions.
                        if let Some(control) = self.controls.get(&term.control) {
                            let site = if side == SnapshotSide::Before {
                                &control.before
                            } else {
                                &control.after
                            };
                            if site.as_ref().is_some_and(|site| {
                                self.controls.values().any(|other| {
                                    other.id != control.id
                                        && (if side == SnapshotSide::Before {
                                            &other.before
                                        } else {
                                            &other.after
                                        })
                                        .as_ref()
                                        .is_some_and(
                                            |other_site| other_site.operation == site.operation,
                                        )
                                })
                            }) {
                                expression.push_str(&format!(" [guard #{}]", control.id.0));
                            }
                        }
                        expression
                    })
                    .collect::<Vec<_>>()
                    .join(" && ");
                if clauses.len() > 1 && clause.terms.len() > 1 {
                    format!("({expression})")
                } else {
                    expression
                }
            })
            .collect::<Vec<_>>()
            .join(" || ")
    }

    fn source(&self, id: LogicalNodeId, side: SnapshotSide) -> String {
        self.source_label(id, side, true)
    }

    fn source_label(&self, id: LogicalNodeId, side: SnapshotSide, located: bool) -> String {
        let (before, after) = self.sites(id);
        let site = if side == SnapshotSide::Before {
            before
        } else {
            after
        };
        let Some(site) = site else {
            return format!("source #{} (absent or position unresolved)", id.0);
        };
        let duplicate = self.sources.values().any(|other| {
            other.id != id
                && source_site(other, side).is_some_and(|other| other.operation == site.operation)
        });
        format!(
            "{}{}{}",
            operation(site),
            if duplicate {
                format!(" [source #{}]", id.0)
            } else {
                String::new()
            },
            if located {
                format!(" ({})", location(site))
            } else {
                String::new()
            }
        )
    }

    fn definition(&self, id: LogicalNodeId, side: SnapshotSide) -> String {
        let mut label = self.source(id, side);
        if let Some(source) = self.sources.get(&id) {
            let guard = assignment_guard(source, side);
            if source.role == SourceRole::Writer && !always(guard) {
                label.push_str(&format!(" enabled under {}", self.clauses(guard, side)));
            }
        }
        label
    }

    fn selected(
        &self,
        state: Option<&ObservationState>,
        ids: &BTreeSet<LogicalNodeId>,
        side: SnapshotSide,
    ) -> String {
        let Some(state) = state else {
            return "observation absent".into();
        };
        let mut labels: Vec<_> = state
            .sources
            .intersection(ids)
            .map(|id| {
                let mut label = self.source(*id, side);
                let selection = state
                    .selections
                    .iter()
                    .find(|selection| selection.source == *id);
                let clauses = selection.and_then(|selection| selection.clauses.as_ref());
                if !always(clauses) {
                    label.push_str(&format!(" selected under {}", self.clauses(clauses, side)));
                }
                label
            })
            .collect();
        if !state.exhaustive {
            labels.push("other sources unresolved".into());
        }
        if labels.is_empty() {
            "no established sources at this input".into()
        } else {
            labels.join("; ")
        }
    }

    fn control_positions(&self, ids: &BTreeSet<LogicalNodeId>, lines: &mut Vec<String>) {
        for side in [SnapshotSide::Before, SnapshotSide::After] {
            let clauses: Vec<_> = ids
                .iter()
                .filter_map(|id| self.sources.get(id))
                .filter_map(|source| assignment_guard(source, side))
                .collect();
            let guards: BTreeSet<_> = clauses
                .into_iter()
                .flat_map(|clauses| clauses.iter())
                .flat_map(|clause| &clause.terms)
                .map(|term| term.control)
                .collect();
            for guard in guards {
                let (before, after) = self.sites(guard);
                let site = if side == SnapshotSide::Before {
                    before
                } else {
                    after
                };
                if let Some(site) = site {
                    // A guard on the writer's line is already explicitly located.
                    if !ids
                        .iter()
                        .filter_map(|id| self.sources.get(id))
                        .filter_map(|source| source_site(source, side))
                        .any(|writer| location(writer) == location(site))
                    {
                        lines.push(self.control_position(guard, side, site));
                    }
                } else {
                    lines.push(format!(
                        "  {side:?} control #{}: position unresolved",
                        guard.0
                    ));
                }
            }
        }
    }

    fn control_position(&self, id: LogicalNodeId, side: SnapshotSide, site: &SourceSite) -> String {
        format!(
            "  {side:?} control #{}: {} ({})",
            id.0,
            site.operation,
            location(site)
        )
    }

    fn copy(&self, id: LogicalNodeId) -> bool {
        let Some(source) = self.sources.get(&id) else {
            return false;
        };
        if source.role != SourceRole::Writer
            || source.before.is_none()
            || source.after.is_none()
            || self.changed_expression(id)
            || !self.report.comparison_complete
            || source.before_upstream.len() != 1
            || source.before_upstream != source.after_upstream
            || !always(source.before_assignment_guard.as_ref())
            || !always(source.after_assignment_guard.as_ref())
        {
            return false;
        }
        let origin = *source.before_upstream.iter().next().unwrap();
        if !self
            .sources
            .get(&origin)
            .is_some_and(|source| matches!(source.role, SourceRole::Writer | SourceRole::Input))
        {
            return false;
        }
        let observations: Vec<_> = self
            .report
            .observations
            .iter()
            .filter(|observation| observation.id == id)
            .collect();
        // Elide intermediate copies only; an otherwise unused copy is itself
        // the final established use and must remain identifiable.
        let has_consumer = self.report.observations.iter().any(|observation| {
            observation.id != id
                && observation
                    .before_state
                    .as_ref()
                    .zip(observation.after_state.as_ref())
                    .is_some_and(|(before, after)| {
                        before.sources.contains(&id) && after.sources.contains(&id)
                    })
        });
        has_consumer
            && !observations.is_empty()
            && observations.iter().all(|observation| {
                observation.projection == "value"
                    && [
                        observation.before_state.as_ref(),
                        observation.after_state.as_ref(),
                    ]
                    .into_iter()
                    .all(|state| {
                        state.is_some_and(|state| {
                            state.exhaustive
                                && state.sources == source.before_upstream
                                && always(state.use_guard.as_ref())
                                && state.precedence.is_empty()
                                && state
                                    .selections
                                    .iter()
                                    .all(|selection| always(selection.clauses.as_ref()))
                        })
                    })
            })
    }

    fn use_label(&self, observation: &Observation) -> String {
        let before = observation.before.as_ref();
        let after = observation.after.as_ref();
        match (before, after) {
            (Some(before), Some(after)) if before.operation == after.operation => format!(
                "{} ({}; {})",
                operation(after),
                input_role(&observation.projection),
                if location(before) == location(after) {
                    format!("before/after {}", location(after))
                } else {
                    format!("before {}; after {}", location(before), location(after))
                }
            ),
            (Some(before), Some(after)) => format!(
                "{} -> {} ({}; before {}; after {})",
                operation(before),
                operation(after),
                input_role(&observation.projection),
                location(before),
                location(after)
            ),
            (Some(site), None) => format!(
                "{} ({}; before {})",
                operation(site),
                input_role(&observation.projection),
                location(site)
            ),
            (None, Some(site)) => format!(
                "{} ({}; after {})",
                operation(site),
                input_role(&observation.projection),
                location(site)
            ),
            (None, None) => format!(
                "observation #{} ({}; position unresolved)",
                observation.id.0, observation.projection
            ),
        }
    }

    fn reaching_rules(
        &self,
        observation: &Observation,
        roots: &BTreeSet<LogicalNodeId>,
    ) -> ReachingRules {
        roots
            .iter()
            .filter_map(|root| {
                let source = self.sources.get(root)?;
                if source.role != SourceRole::Writer {
                    return None;
                }
                let selection = |state: Option<&ObservationState>| {
                    state
                        .and_then(|state| {
                            state
                                .selections
                                .iter()
                                .find(|selection| selection.source == *root)
                        })
                        .map(|selection| selection.clauses.clone())
                };
                let before = selection(observation.before_state.as_ref());
                let after = selection(observation.after_state.as_ref());
                (before.as_ref().is_some_and(|clauses| {
                    clauses.as_ref() != source.before_assignment_guard.as_ref()
                }) || after.as_ref().is_some_and(|clauses| {
                    clauses.as_ref() != source.after_assignment_guard.as_ref()
                }))
                .then_some((*root, before, after))
            })
            .collect()
    }

    fn flow(
        &self,
        members: &BTreeSet<ObservationKey>,
        roots: &BTreeSet<LogicalNodeId>,
        lines: &mut Vec<String>,
    ) {
        let mut conditions: BTreeMap<FlowConditions, Vec<&Observation>> = BTreeMap::new();
        for member in members {
            let observation = self.observations[member];
            if roots.contains(&observation.id) || self.copy(observation.id) {
                continue;
            }
            conditions
                .entry((
                    observation
                        .before_state
                        .as_ref()
                        .and_then(|state| state.use_guard.clone()),
                    observation
                        .after_state
                        .as_ref()
                        .and_then(|state| state.use_guard.clone()),
                    observation
                        .before_state
                        .as_ref()
                        .is_some_and(|state| !state.exhaustive),
                    observation
                        .after_state
                        .as_ref()
                        .is_some_and(|state| !state.exhaustive),
                    self.reaching_rules(observation, roots),
                ))
                .or_default()
                .push(observation);
        }
        for ((before, after, before_partial, after_partial, reaching), mut observations) in
            conditions
        {
            observations.sort_by_key(|observation| {
                let site = observation.after.as_ref().or(observation.before.as_ref());
                (
                    site.map(|site| (&site.span.path, site.span.byte_start)),
                    observation.id,
                    &observation.projection,
                )
            });
            lines.push(format!(
                "Affected uses: {}",
                observations
                    .iter()
                    .map(|observation| self.use_label(observation))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
            if !always(before.as_ref()) || !always(after.as_ref()) {
                let before_label = if observations
                    .iter()
                    .all(|observation| observation.before.is_none())
                {
                    "absent".into()
                } else {
                    self.clauses(before.as_ref(), SnapshotSide::Before)
                };
                let after_label = if observations
                    .iter()
                    .all(|observation| observation.after.is_none())
                {
                    "absent".into()
                } else {
                    self.clauses(after.as_ref(), SnapshotSide::After)
                };
                lines.push(format!(
                    "  Use guard: before {before_label}; after {after_label}"
                ));
            }
            for (root, before, after) in &reaching {
                let (before_site, after_site) = self.sites(*root);
                let label = after_site
                    .or(before_site)
                    .map(operation)
                    .unwrap_or("unresolved source");
                let rule = |clauses: &Option<Option<Clauses>>, side| {
                    if let Some(clauses) = clauses {
                        self.clauses(clauses.as_ref(), side)
                    } else {
                        "not a source at this input".into()
                    }
                };
                lines.push(format!(
                    "  Reaching rule for {label}: before {}; after {}",
                    rule(before, SnapshotSide::Before),
                    rule(after, SnapshotSide::After)
                ));
            }
            if before_partial || after_partial {
                lines.push(format!(
                    "  Sources unresolved: before {before_partial}; after {after_partial}"
                ));
            }
            let guards: BTreeSet<_> = before
                .iter()
                .chain(after.iter())
                .chain(reaching.iter().flat_map(|(_, before, after)| {
                    before
                        .iter()
                        .chain(after.iter())
                        .filter_map(|clauses| clauses.as_ref())
                }))
                .flat_map(|clauses| clauses.iter())
                .flat_map(|clause| &clause.terms)
                .map(|term| term.control)
                .collect();
            for guard in guards {
                for (side, site) in [
                    (SnapshotSide::Before, self.sites(guard).0),
                    (SnapshotSide::After, self.sites(guard).1),
                ] {
                    if let Some(site) = site {
                        // Skip guard positions already shown elsewhere in this block.
                        let line = self.control_position(guard, side, site);
                        if !lines.contains(&line) {
                            lines.push(line);
                        }
                    } else {
                        let used = (if side == SnapshotSide::Before {
                            &before
                        } else {
                            &after
                        })
                        .iter()
                        .chain(reaching.iter().filter_map(|(_, before, after)| {
                            (if side == SnapshotSide::Before {
                                before
                            } else {
                                after
                            })
                            .as_ref()
                            .and_then(|clauses| clauses.as_ref())
                        }))
                        .any(|clauses| {
                            clauses
                                .iter()
                                .any(|clause| clause.terms.iter().any(|term| term.control == guard))
                        });
                        if used {
                            let line =
                                format!("  {side:?} control #{}: position unresolved", guard.0);
                            if !lines.contains(&line) {
                                lines.push(line);
                            }
                        }
                    }
                }
            }
        }
    }

    fn short(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for (change, members) in self.groups() {
            let roots = match &change {
                Change::Expression(ids) | Change::Guards(ids) => ids.clone(),
                Change::ControlGuards(causes) => self.guard_writers(causes, &members),
                _ => BTreeSet::new(),
            };
            if matches!(change, Change::Expression(_))
                && members.iter().all(|member| roots.contains(&member.0))
                && roots.iter().all(|id| self.unused_write(*id))
            {
                continue;
            }
            let observation = members
                .iter()
                .filter_map(|member| self.observations.get(member).copied())
                .max_by_key(|observation| {
                    (
                        !roots.contains(&observation.id),
                        observation
                            .before_state
                            .iter()
                            .chain(observation.after_state.iter())
                            .map(|state| state.sources.len())
                            .max()
                            .unwrap_or(0),
                    )
                })
                .unwrap();
            match &change {
                Change::Expression(_) | Change::Guards(_) | Change::ControlGuards(_) => {
                    let ids = &roots;
                    lines.push(if !matches!(change, Change::Expression(_)) {
                        "Selection changed:".into()
                    } else {
                        "Source expression changed:".into()
                    });
                    for side in [SnapshotSide::Before, SnapshotSide::After] {
                        lines.push(format!(
                            "  {side:?}: {}",
                            ids.iter()
                                .map(|id| self.definition(*id, side))
                                .collect::<Vec<_>>()
                                .join("; ")
                        ));
                    }
                    if matches!(change, Change::Expression(_)) {
                        let roles: BTreeSet<_> = members
                            .iter()
                            .filter(|member| roots.contains(&member.0))
                            .filter(|member| {
                                self.sources.get(&member.0).is_some_and(|source| {
                                    source.role != SourceRole::Writer
                                        || !self.unused_write(source.id)
                                })
                            })
                            .map(|member| input_role(&member.1))
                            .collect();
                        if !roles.is_empty() {
                            lines.push(format!(
                                "  Affected input at changed producer: {}",
                                roles.into_iter().collect::<Vec<_>>().join("; ")
                            ));
                        }
                    }
                    self.control_positions(ids, &mut lines);
                    if !matches!(change, Change::Expression(_)) {
                        self.fallback(observation, ids, &mut lines);
                    }
                }
                Change::Sources(before, after, _, _) => {
                    lines.push("Sources changed:".into());
                    let ids = before.union(after).copied().collect();
                    for (side, state) in [
                        (SnapshotSide::Before, observation.before_state.as_ref()),
                        (SnapshotSide::After, observation.after_state.as_ref()),
                    ] {
                        lines.push(format!("  {side:?}: {}", self.selected(state, &ids, side)));
                        if let Some(state) = state {
                            let other = if side == SnapshotSide::Before {
                                &observation.after_state
                            } else {
                                &observation.before_state
                            };
                            for rule in state.precedence.iter().filter(|rule| {
                                !other
                                    .as_ref()
                                    .is_some_and(|other| other.precedence.contains(rule))
                            }) {
                                lines.push(format!(
                                    "  {side:?} precedence: {} overrides {} when both writes apply",
                                    self.source_label(rule.later, side, false),
                                    self.source_label(rule.earlier, side, false)
                                ));
                            }
                        }
                    }
                    for removed in before.difference(after) {
                        if self.sources.get(removed).is_some_and(|source| {
                            source.role == SourceRole::Writer && source.after.is_some()
                        }) {
                            lines.push(format!(
                                "  Retained in code, no longer reaches this input: after {}",
                                self.source(*removed, SnapshotSide::After)
                            ));
                        }
                    }
                    self.control_positions(&ids, &mut lines);
                }
                Change::Selection(_) => {
                    lines.push("Selection changed:".into());
                    let ids: BTreeSet<_> = observation
                        .before_state
                        .iter()
                        .chain(observation.after_state.iter())
                        .flat_map(|state| &state.sources)
                        .copied()
                        .collect();
                    let changed: BTreeSet<_> = ids
                        .iter()
                        .copied()
                        .filter(|id| {
                            let selection = |state: Option<&ObservationState>| {
                                state
                                    .and_then(|state| {
                                        state
                                            .selections
                                            .iter()
                                            .find(|selection| selection.source == *id)
                                    })
                                    .map(|selection| selection.clauses.clone())
                            };
                            selection(observation.before_state.as_ref())
                                != selection(observation.after_state.as_ref())
                        })
                        .collect();
                    for (side, state) in [
                        (SnapshotSide::Before, observation.before_state.as_ref()),
                        (SnapshotSide::After, observation.after_state.as_ref()),
                    ] {
                        if !changed.is_empty() {
                            lines.push(format!(
                                "  {side:?}: {}",
                                self.selected(state, &changed, side)
                            ));
                        }
                        if let Some(state) = state {
                            let other = if side == SnapshotSide::Before {
                                &observation.after_state
                            } else {
                                &observation.before_state
                            };
                            for rule in state.precedence.iter().filter(|rule| {
                                !other
                                    .as_ref()
                                    .is_some_and(|other| other.precedence.contains(rule))
                            }) {
                                lines.push(format!(
                                    "  {side:?} precedence: {} overrides {} when both writes apply",
                                    self.source_label(
                                        rule.later,
                                        side,
                                        !changed.contains(&rule.later)
                                    ),
                                    self.source_label(
                                        rule.earlier,
                                        side,
                                        !changed.contains(&rule.earlier)
                                    )
                                ));
                            }
                        }
                    }
                    self.fallback(observation, &changed, &mut lines);
                    self.control_positions(&ids, &mut lines);
                }
                Change::Observation(kind, _) => {
                    let label = match kind {
                        FindingKind::ObservationAdded => "Added observation",
                        FindingKind::ObservationRemoved => "Removed observation",
                        FindingKind::ObservationFlowAdded => {
                            "Observation now carries the selected binding"
                        }
                        _ => "Observation no longer carries the selected binding",
                    };
                    lines.push(format!("{label}:"));
                    for (side, state) in [
                        (SnapshotSide::Before, observation.before_state.as_ref()),
                        (SnapshotSide::After, observation.after_state.as_ref()),
                    ] {
                        if let Some(state) = state {
                            lines.push(format!(
                                "  {side:?} sources: {}",
                                self.selected(Some(state), &state.sources, side)
                            ));
                        }
                    }
                }
            }
            self.flow(&members, &roots, &mut lines);
        }
        if lines.is_empty() {
            lines.push("No established changes at uses within the declared scope.".into());
        }
        if self
            .report
            .findings
            .iter()
            .any(|finding| finding.kind == FindingKind::ComparisonUnresolved)
        {
            lines.push("Comparison unresolved at one or more aligned operations.".into());
        }
        lines
    }

    fn unused_write(&self, id: LogicalNodeId) -> bool {
        let Some(source) = self.sources.get(&id) else {
            return false;
        };
        if source.role != SourceRole::Writer {
            return false;
        }
        let selector = &self.report.selected_binding;
        let selected_declaration = [source.before.as_ref(), source.after.as_ref()]
            .into_iter()
            .flatten()
            .any(|site| {
                site.node.snapshot == selector.snapshot
                    && site.span.path == selector.declaration.path
                    && site.span.byte_start <= selector.declaration.byte_start
                    && site.span.byte_end >= selector.declaration.byte_end
            });
        // A changed declaration's RHS is not a downstream use of its own value.
        // Keep local consumers with binding/input origins conservatively visible.
        selected_declaration
            || source
                .before_upstream
                .iter()
                .chain(&source.after_upstream)
                .all(|parent| {
                    self.sources.get(parent).is_some_and(|parent| {
                        matches!(parent.role, SourceRole::Literal | SourceRole::Computation)
                    })
                })
    }

    fn fallback(
        &self,
        observation: &Observation,
        changed: &BTreeSet<LogicalNodeId>,
        lines: &mut Vec<String>,
    ) {
        let mut labels = Vec::new();
        for id in observation
            .before_state
            .iter()
            .chain(observation.after_state.iter())
            .flat_map(|state| &state.sources)
            .copied()
            .collect::<BTreeSet<_>>()
            .difference(changed)
        {
            let Some(source) = self.sources.get(id) else {
                continue;
            };
            if source.role == SourceRole::Writer
                && source.before.is_some()
                && source.after.is_some()
                && always(source.before_assignment_guard.as_ref())
                && always(source.after_assignment_guard.as_ref())
            {
                labels.push(
                    if source
                        .before
                        .as_ref()
                        .zip(source.after.as_ref())
                        .is_some_and(|(before, after)| {
                            before.operation == after.operation
                                && location(before) == location(after)
                        })
                    {
                        format!("before/after {}", self.source(*id, SnapshotSide::Before))
                    } else {
                        format!(
                            "before {}; after {}",
                            self.source(*id, SnapshotSide::Before),
                            self.source(*id, SnapshotSide::After)
                        )
                    },
                );
            }
        }
        if !labels.is_empty() {
            lines.push(format!("  Fallback: {}", labels.join("; ")));
        }
    }

    fn details(&self, lines: &mut Vec<String>) {
        lines.push("Sources and dependencies:".into());
        for source in self.sources.values() {
            for side in [SnapshotSide::Before, SnapshotSide::After] {
                if source_site(source, side).is_none() {
                    continue;
                }
                let parents = upstream(source, side);
                lines.push(format!(
                    "  {side:?}: {}{}",
                    self.definition(source.id, side),
                    if parents.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " <- {}",
                            parents
                                .iter()
                                .map(|id| self.source(*id, side))
                                .collect::<Vec<_>>()
                                .join("; ")
                        )
                    }
                ));
            }
        }
        for observation in self.observations.values() {
            lines.push(format!("Observation: {}", self.use_label(observation)));
            for (side, state) in [
                (SnapshotSide::Before, observation.before_state.as_ref()),
                (SnapshotSide::After, observation.after_state.as_ref()),
            ] {
                if let Some(state) = state {
                    lines.push(format!(
                        "  {side:?}: {}; use guard {}",
                        self.selected(Some(state), &state.sources, side),
                        self.clauses(state.use_guard.as_ref(), side)
                    ));
                    for rule in &state.precedence {
                        lines.push(format!(
                            "  {side:?} precedence: {} overrides {} when both writes apply",
                            self.source(rule.later, side),
                            self.source(rule.earlier, side)
                        ));
                    }
                }
            }
        }
    }

    fn notices(&self, lines: &mut Vec<String>) {
        let diagnostics: BTreeSet<_> = self
            .full
            .diagnostics
            .iter()
            .map(|diagnostic| {
                let position = diagnostic
                    .span
                    .as_ref()
                    .map(|span| {
                        format!(
                            " ({}{}:{})",
                            diagnostic
                                .snapshot
                                .as_ref()
                                .map(|snapshot| format!("{:?} ", snapshot.side))
                                .unwrap_or_default(),
                            span.path,
                            span.start_line
                        )
                    })
                    .unwrap_or_default();
                format!(
                    "Unknown: {:?}: {}{}",
                    diagnostic.code, diagnostic.message, position
                )
            })
            .collect();
        lines.extend(diagnostics);
        for assumption in &self.report.entry_assumptions {
            if let Some(value) = &assumption.value {
                lines.push(format!("Entry assumption: {} = {value:?}", assumption.name));
            }
        }
        for (side, extent) in [
            (SnapshotSide::Before, &self.full.flow_extent.before),
            (SnapshotSide::After, &self.full.flow_extent.after),
        ] {
            if extent.upstream != Coverage::Complete || extent.downstream != Coverage::Complete {
                lines.push(format!(
                    "{side:?} coverage: upstream {:?}; downstream {:?}",
                    extent.upstream, extent.downstream
                ));
            }
        }
        let stats = &self.full.stats;
        if !stats.limits_hit.is_empty()
            || stats.graph_truncated
            || stats.witnesses_truncated
            || stats.witnesses_omitted > 0
        {
            lines.push(format!(
                "Limits: {:?}; graph truncated {}; witnesses truncated {}; witnesses omitted {}",
                stats.limits_hit,
                stats.graph_truncated,
                stats.witnesses_truncated,
                stats.witnesses_omitted
            ));
        }
        let before = &self.full.flow_extent.before.declared_scope;
        let after = &self.full.flow_extent.after.declared_scope;
        let common_caller_boundary = self.report.source_boundaries.is_empty()
            && self.report.sink_boundaries == BTreeSet::from(["caller_scope".into()])
            && !self.full.flow_extent.before.value_lifecycle_closed
            && !self.full.flow_extent.after.value_lifecycle_closed;
        lines.push(format!(
            "Scope: {}; analysis {}; source comparison {}{}{}.",
            if before == after {
                before.clone()
            } else {
                format!("before {before}; after {after}")
            },
            match self.report.analysis_coverage {
                Completeness::CompleteForQuery => "complete",
                Completeness::Partial => "partial",
                Completeness::Unsupported => "unsupported",
            },
            if self.report.comparison_complete {
                "complete"
            } else {
                "unresolved"
            },
            if self.report.presentation_complete && self.report.omitted_groups == 0 {
                String::new()
            } else {
                format!(
                    "; presentation {}{}",
                    if self.report.presentation_complete {
                        "complete"
                    } else {
                        "partial"
                    },
                    if self.report.omitted_groups > 0 {
                        format!(
                            " ({} groups omitted; see full evidence)",
                            self.report.omitted_groups
                        )
                    } else {
                        String::new()
                    }
                )
            },
            if common_caller_boundary {
                "; caller continuation outside scope"
            } else {
                ""
            }
        ));
        if !common_caller_boundary
            && (!self.report.source_boundaries.is_empty()
                || !self.report.sink_boundaries.is_empty()
                || !self.full.flow_extent.before.value_lifecycle_closed
                || !self.full.flow_extent.after.value_lifecycle_closed)
        {
            lines.push(format!(
                "Boundaries: sources {:?}; sinks {:?}; lifecycle before {}, after {}",
                self.report.source_boundaries,
                self.report.sink_boundaries,
                if self.full.flow_extent.before.value_lifecycle_closed {
                    "closed within scope"
                } else {
                    "open"
                },
                if self.full.flow_extent.after.value_lifecycle_closed {
                    "closed within scope"
                } else {
                    "open"
                }
            ));
        }
        lines.push(format!(
            "Evidence: {} ({})",
            self.report.evidence_file, self.report.full_report_id
        ));
    }
}
