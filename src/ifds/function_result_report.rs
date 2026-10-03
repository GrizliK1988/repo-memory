//! Versioned full evidence and compact presentation for function results.

use super::compact::EvidenceRef;
use super::function_result_comparison::*;
use super::function_results::*;
use super::model::{Coverage, SnapshotId, SnapshotSide, SourceSpan};
use super::snapshots::{AnalysisEnvironment, SnapshotProvider};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

mod attribution;
mod causes;
mod grouping;
mod selection;
mod text;
mod values;
use attribution::*;
use causes::*;
pub use causes::{ControlCauseSide, FunctionResultCause, FunctionResultCauseKind};
use grouping::*;
use selection::*;
use text::*;
use values::*;
pub use values::{DependencyEvidence, PresentedResult, ReturnEvidence};

pub const FUNCTION_RESULT_REPORT_SCHEMA_VERSION: u32 = 3;
pub const FUNCTION_RESULT_COMPACT_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionResultFinding {
    pub assessment: ResultAssessment,
    pub before_result: Option<String>,
    pub after_result: Option<String>,
    pub regions: Vec<usize>,
    pub control_changed: bool,
    pub value_dependency_changed: bool,
    pub return_structure_changed: bool,
    pub before_locations: Vec<SourceSpan>,
    pub after_locations: Vec<SourceSpan>,
    pub attribution_certain: bool,
    pub cause_refs: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionResultReport {
    pub schema_version: u32,
    pub analysis: FunctionResultAnalysis,
    pub comparison: FunctionResultComparison,
    pub findings: Vec<FunctionResultFinding>,
    pub causes: Vec<FunctionResultCause>,
}

impl FunctionResultReport {
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        let report: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if report.schema_version != FUNCTION_RESULT_REPORT_SCHEMA_VERSION
            || report.analysis.schema_version != FUNCTION_RESULT_SCHEMA_VERSION
        {
            return Err(format!(
                "unsupported function result report versions {}/{}",
                report.schema_version, report.analysis.schema_version
            ));
        }
        report.validate_control_causes()?;
        Ok(report)
    }

    fn validate_control_causes(&self) -> Result<(), String> {
        let expected = control_change_causes(&self.analysis, &self.comparison);
        if self.causes != expected {
            return Err("function control causes do not match dependency evidence".into());
        }
        let mut expected_findings = findings(&self.analysis, &self.comparison);
        link_control_causes(&mut expected_findings, &expected);
        if self.findings != expected_findings {
            return Err("function findings do not match attribution evidence".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionChoiceConditions {
    pub result: PresentedResult,
    /// These regions are relative to their enclosing finding's common context.
    pub before: Vec<BooleanRegion>,
    pub after: Vec<BooleanRegion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactResultFinding {
    pub context: BooleanRegion,
    pub choices: Vec<FunctionChoiceConditions>,
    /// The same-input effect regions are relative to `context`.
    pub effect: Vec<BooleanRegion>,
    pub effects: Vec<CompactResultRegion>,
    pub assessment: ResultAssessment,
    pub control_changed: bool,
    pub value_dependency_changed: bool,
    pub return_structure_changed: bool,
    pub evidence: Vec<EvidenceRef>,
    pub attribution_certain: bool,
    pub cause_refs: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactResultRegion {
    pub region: BooleanRegion,
    pub before_result: Option<PresentedResult>,
    pub after_result: Option<PresentedResult>,
    pub assessment: ResultAssessment,
    pub reason: Option<String>,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactSideFlow {
    pub flow: SnapshotResultFlow,
    pub result: Option<PresentedResult>,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionReportCoverage {
    pub before_analysis: Option<Coverage>,
    pub after_analysis: Option<Coverage>,
    pub source_alignment: Coverage,
    pub input_mapping: Coverage,
    pub result_comparison: Coverage,
    pub presentation: Coverage,
    pub analysis_limits_hit: BTreeSet<String>,
    pub comparison_limits_hit: BTreeSet<String>,
    pub omitted_groups: usize,
    pub caller_continuation_open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionResultCompactReport {
    pub schema_version: u32,
    pub full_report_id: String,
    pub evidence_file: String,
    pub snapshots: BTreeSet<SnapshotId>,
    pub selected_function: FunctionSelector,
    pub counterpart: Option<FunctionSelector>,
    pub input_names: Vec<String>,
    pub before_entry: FunctionEntry,
    pub after_entry: FunctionEntry,
    pub function_presence: FunctionPresence,
    pub common_domain_status: CommonDomainStatus,
    pub common_domain: Vec<BooleanRegion>,
    pub before_only_domain: Vec<BooleanRegion>,
    pub after_only_domain: Vec<BooleanRegion>,
    pub unresolved_domain: Vec<BooleanRegion>,
    pub before_flow: Vec<CompactSideFlow>,
    pub after_flow: Vec<CompactSideFlow>,
    pub findings: Vec<CompactResultFinding>,
    pub equal_regions: Vec<CompactResultRegion>,
    pub unknown_regions: Vec<CompactResultRegion>,
    pub unchanged_choices: Vec<PresentedResult>,
    pub coverage: FunctionReportCoverage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FunctionPresentationOptions {
    pub max_groups: usize,
}

impl Default for FunctionPresentationOptions {
    fn default() -> Self {
        Self {
            max_groups: 100_000,
        }
    }
}

fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn report_id(full: &FunctionResultReport) -> String {
    let bytes = serde_json::to_vec(full).expect("function result report serializes");
    format!("ifds-function-{:016x}", stable_hash(&bytes))
}

fn observation<'a>(
    side: &'a Option<FunctionSnapshotResult>,
    reference: &ResultObservationRef,
) -> Option<(usize, &'a ResultObservation)> {
    side.as_ref()?
        .observations
        .iter()
        .enumerate()
        .find(|(_, item)| item.site == reference.site && item.path_index == reference.path_index)
}

fn locations(
    side: &Option<FunctionSnapshotResult>,
    refs: &[ResultObservationRef],
    include_guards: bool,
) -> Vec<SourceSpan> {
    let mut spans = BTreeSet::new();
    for reference in refs {
        if let Some((_, item)) = observation(side, reference) {
            spans.insert((
                item.span.path.clone(),
                item.span.byte_start,
                item.span.byte_end,
                item.span.start_line,
                item.span.end_line,
            ));
            if include_guards {
                for guard in &item.guards {
                    if let Some(span) = &guard.span {
                        spans.insert((
                            span.path.clone(),
                            span.byte_start,
                            span.byte_end,
                            span.start_line,
                            span.end_line,
                        ));
                    }
                }
            }
        }
    }
    spans
        .into_iter()
        .map(
            |(path, byte_start, byte_end, start_line, end_line)| SourceSpan {
                path,
                byte_start,
                byte_end,
                start_line,
                end_line,
            },
        )
        .collect()
}

fn findings(
    analysis: &FunctionResultAnalysis,
    comparison: &FunctionResultComparison,
) -> Vec<FunctionResultFinding> {
    let mut groups: BTreeMap<String, FunctionResultFinding> = BTreeMap::new();
    for (index, region) in comparison.regions.iter().enumerate() {
        if region.assessment == ResultAssessment::Equal
            && !region.control_changed
            && !region.value_dependency_changed
            && !region.return_structure_changed
        {
            continue;
        }
        let key = format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            region.assessment,
            region.before_result,
            region.after_result,
            region.control_changed,
            region.value_dependency_changed,
            region.return_structure_changed,
            region.unknown_reason
        );
        let finding = groups.entry(key).or_insert_with(|| FunctionResultFinding {
            assessment: region.assessment,
            before_result: region.before_result.clone(),
            after_result: region.after_result.clone(),
            regions: Vec::new(),
            control_changed: region.control_changed,
            value_dependency_changed: region.value_dependency_changed,
            return_structure_changed: region.return_structure_changed,
            before_locations: Vec::new(),
            after_locations: Vec::new(),
            attribution_certain: false,
            cause_refs: Vec::new(),
        });
        finding.regions.push(index);
        finding.before_locations.extend(locations(
            &analysis.before,
            &region.before_observations,
            region.control_changed,
        ));
        finding.after_locations.extend(locations(
            &analysis.after,
            &region.after_observations,
            region.control_changed,
        ));
    }
    let mut output: Vec<_> = groups.into_values().collect();
    for finding in &mut output {
        finding.before_locations.sort_by(|a, b| {
            (&a.path, a.byte_start, a.byte_end).cmp(&(&b.path, b.byte_start, b.byte_end))
        });
        finding.before_locations.dedup();
        finding.after_locations.sort_by(|a, b| {
            (&a.path, a.byte_start, a.byte_end).cmp(&(&b.path, b.byte_start, b.byte_end))
        });
        finding.after_locations.dedup();
        if unique_dependency_edit(analysis, comparison, &finding.regions).is_some() {
            finding.attribution_certain = true;
            continue;
        }
        if !finding.control_changed {
            let before_refs: BTreeSet<_> = finding
                .regions
                .iter()
                .flat_map(|index| &comparison.regions[*index].before_observations)
                .collect();
            let after_refs: BTreeSet<_> = finding
                .regions
                .iter()
                .flat_map(|index| &comparison.regions[*index].after_observations)
                .collect();
            if let (Some(before), Some(after), 1, 1) = (
                &analysis.before,
                &analysis.after,
                before_refs.len(),
                after_refs.len(),
            ) {
                let before_index = before.observations.iter().position(|item| {
                    before_refs.iter().any(|reference| {
                        reference.site == item.site && reference.path_index == item.path_index
                    })
                });
                let after_index = after.observations.iter().position(|item| {
                    after_refs.iter().any(|reference| {
                        reference.site == item.site && reference.path_index == item.path_index
                    })
                });
                if let (Some(a), Some(b)) = (before_index, after_index) {
                    let before_item = &before.observations[a];
                    let after_item = &after.observations[b];
                    finding.attribution_certain = before.observations.len()
                        == after.observations.len()
                        && a == b
                        && before_item.span.byte_start == after_item.span.byte_start
                        && before_item.span.start_line == after_item.span.start_line
                        && std::mem::discriminant(&before_item.kind)
                            == std::mem::discriminant(&after_item.kind)
                        && before_item.guards.len() == after_item.guards.len()
                        && before_item
                            .guards
                            .iter()
                            .zip(&after_item.guards)
                            .all(|(a, b)| a.outcome == b.outcome)
                        && match (&before_item.value, &after_item.value) {
                            (
                                ResultValue::Expression { text: a, .. },
                                ResultValue::Expression { text: b, .. },
                            ) => a != b,
                            (ResultValue::Undefined, ResultValue::Undefined) => false,
                            _ => true,
                        };
                }
            }
        }
    }
    output
}

fn canonicalize_analysis(mut analysis: FunctionResultAnalysis) -> FunctionResultAnalysis {
    for side in [&mut analysis.before, &mut analysis.after]
        .into_iter()
        .flatten()
    {
        side.observations
            .sort_by(|a, b| (&a.site, a.path_index).cmp(&(&b.site, b.path_index)));
    }
    analysis
        .input_correspondence
        .sort_by_key(|input| input.position);
    analysis
}

pub fn assemble_function_result_report(analysis: FunctionResultAnalysis) -> FunctionResultReport {
    let analysis = canonicalize_analysis(analysis);
    let comparison = compare_function_results(&analysis);
    let mut findings = findings(&analysis, &comparison);
    let causes = control_change_causes(&analysis, &comparison);
    link_control_causes(&mut findings, &causes);
    FunctionResultReport {
        schema_version: FUNCTION_RESULT_REPORT_SCHEMA_VERSION,
        analysis,
        comparison,
        findings,
        causes,
    }
}

pub fn assemble_function_result_report_with_comparison_limit(
    analysis: FunctionResultAnalysis,
    max_assignments: usize,
) -> FunctionResultReport {
    let analysis = canonicalize_analysis(analysis);
    let comparison = compare_function_results_with_limit(&analysis, max_assignments);
    let mut findings = findings(&analysis, &comparison);
    let causes = control_change_causes(&analysis, &comparison);
    link_control_causes(&mut findings, &causes);
    FunctionResultReport {
        schema_version: FUNCTION_RESULT_REPORT_SCHEMA_VERSION,
        analysis,
        comparison,
        findings,
        causes,
    }
}

fn reference(id: &str, section: &str, index: usize) -> EvidenceRef {
    EvidenceRef {
        report_id: id.into(),
        section: section.into(),
        index,
    }
}

fn compact_flows(
    id: &str,
    section: &str,
    side: &Option<FunctionSnapshotResult>,
    flows: &[SnapshotResultFlow],
) -> Vec<CompactSideFlow> {
    flows
        .iter()
        .map(|flow| CompactSideFlow {
            flow: flow.clone(),
            result: present_result(id, section, side, &flow.observations, &flow.result),
            evidence: flow
                .observations
                .iter()
                .filter_map(|observation_ref| {
                    observation(side, observation_ref)
                        .map(|(index, _)| reference(id, section, index))
                })
                .collect(),
        })
        .collect()
}

fn choice_regions(flows: &[CompactSideFlow], identity: &str) -> Vec<BooleanRegion> {
    let mut regions: Vec<_> = flows
        .iter()
        .filter(|flow| {
            flow.result
                .as_ref()
                .is_some_and(|result| result.identity == identity)
                && flow.flow.completion == Coverage::Complete
        })
        .map(|flow| flow.flow.region.clone())
        .collect();
    regions.sort_by(|a, b| a.values.cmp(&b.values));
    regions.dedup();
    simplify_regions(regions)
}

fn simplify_regions(mut regions: Vec<BooleanRegion>) -> Vec<BooleanRegion> {
    let slots: Vec<u32> = regions
        .iter()
        .flat_map(|region| region.values.keys().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if slots.len() > 12 {
        return regions;
    }
    let covered = |candidate: &BooleanRegion, others: &[BooleanRegion]| {
        (0..(1_usize << slots.len())).all(|mask| {
            let matches = |region: &BooleanRegion| {
                region.values.iter().all(|(slot, value)| {
                    let position = slots.binary_search(slot).expect("listed slot");
                    (mask & (1 << position) != 0) == *value
                })
            };
            !matches(candidate) || others.iter().any(matches)
        })
    };
    let original = regions.clone();
    for region in &mut regions {
        let keys: Vec<_> = region.values.keys().copied().collect();
        for key in keys {
            let mut candidate = region.clone();
            candidate.values.remove(&key);
            if covered(&candidate, &original) {
                *region = candidate;
            }
        }
    }
    regions.sort_by(|a, b| a.values.cmp(&b.values));
    regions.dedup();
    let mut index = 0;
    while index < regions.len() {
        let mut others = regions.clone();
        let candidate = others.remove(index);
        if covered(&candidate, &others) {
            regions.remove(index);
        } else {
            index += 1;
        }
    }
    regions
}

fn common_context(regions: &[BooleanRegion]) -> BooleanRegion {
    let mut values = regions
        .first()
        .map_or_else(BTreeMap::new, |r| r.values.clone());
    values.retain(|slot, value| regions.iter().all(|r| r.values.get(slot) == Some(value)));
    BooleanRegion { values }
}

fn relative(regions: Vec<BooleanRegion>, context: &BooleanRegion) -> Vec<BooleanRegion> {
    regions
        .into_iter()
        .map(|mut region| {
            for slot in context.values.keys() {
                region.values.remove(slot);
            }
            region
        })
        .collect()
}

fn guard_origins(
    full: &FunctionResultReport,
    region: &ComparedResultRegion,
) -> (
    BTreeSet<super::model::NodeId>,
    BTreeSet<super::model::NodeId>,
) {
    let sites = |side: &Option<FunctionSnapshotResult>, refs: &[ResultObservationRef]| {
        refs.iter()
            .filter_map(|reference| observation(side, reference))
            .flat_map(|(_, item)| item.guards.iter().map(|guard| guard.branch.clone()))
            .collect()
    };
    (
        sites(&full.analysis.before, &region.before_observations),
        sites(&full.analysis.after, &region.after_observations),
    )
}

pub fn compact_function_result_report(
    full: &FunctionResultReport,
    options: FunctionPresentationOptions,
) -> FunctionResultCompactReport {
    let id = report_id(full);
    let comparison = &full.comparison;
    let before_flow = compact_flows(
        &id,
        "analysis.before.observations",
        &full.analysis.before,
        &split_result_flows(
            &full.analysis,
            &full.analysis.before,
            &comparison.before_flow,
        ),
    );
    let after_flow = compact_flows(
        &id,
        "analysis.after.observations",
        &full.analysis.after,
        &split_result_flows(&full.analysis, &full.analysis.after, &comparison.after_flow),
    );
    let mut result_choices: BTreeMap<String, PresentedResult> = BTreeMap::new();
    for flow in before_flow.iter().chain(&after_flow) {
        if flow.flow.completion == Coverage::Complete
            && let Some(result) = &flow.result
        {
            result_choices
                .entry(result.identity.clone())
                .and_modify(|target| merge_result(target, result))
                .or_insert_with(|| result.clone());
        }
    }
    let has_changed_result = full.findings.iter().any(|finding| {
        !matches!(
            finding.assessment,
            ResultAssessment::Equal | ResultAssessment::Unknown
        )
    });
    let mut groups: BTreeMap<String, (Vec<CompactResultRegion>, &FunctionResultFinding)> =
        BTreeMap::new();
    let mut affected_choices = BTreeSet::new();
    for finding in &full.findings {
        if finding.assessment == ResultAssessment::Unknown
            || (has_changed_result && finding.assessment == ResultAssessment::Equal)
        {
            continue;
        }
        for index in &finding.regions {
            let effect = compact_region(full, &id, *index);
            let identities: Vec<_> = [&effect.before_result, &effect.after_result]
                .into_iter()
                .map(|result| result.as_ref().map(|result| &result.identity))
                .collect();
            let region = &comparison.regions[*index];
            let origins = guard_origins(full, region);
            let key = serde_json::to_string(&(
                identities,
                origins,
                finding.assessment,
                finding.control_changed,
                finding.value_dependency_changed,
                finding.return_structure_changed,
            ))
            .unwrap();
            if finding.assessment != ResultAssessment::Equal {
                for result in [&effect.before_result, &effect.after_result]
                    .into_iter()
                    .flatten()
                {
                    affected_choices.insert(result.identity.clone());
                }
            }
            groups
                .entry(key)
                .or_insert_with(|| (Vec::new(), finding))
                .0
                .push(effect);
        }
    }
    let mut grouped: Vec<_> = groups.into_values().collect();
    let mut candidates: BTreeMap<String, usize> = BTreeMap::new();
    let mut merged = BTreeSet::new();
    for index in 0..grouped.len() {
        let (effects, finding) = &grouped[index];
        if effects.len() != 1 || !finding.control_changed {
            continue;
        }
        let effect = &effects[0];
        let (Some(before), Some(after)) = (&effect.before_result, &effect.after_result) else {
            continue;
        };
        let mut pair = [&before.identity, &after.identity];
        pair.sort();
        let region = &comparison.regions[effect.evidence.index];
        let origins = guard_origins(full, region);
        let key = serde_json::to_string(&(
            pair,
            origins,
            finding.assessment,
            finding.value_dependency_changed,
            finding.return_structure_changed,
        ))
        .unwrap();
        if let Some(&previous) = candidates.get(&key) {
            if grouped[previous].0.len() == 1
                && symmetric_guard(full, &grouped[previous].0[0], effect)
            {
                let effects = effects.clone();
                grouped[previous].0.extend(effects);
                merged.insert(index);
            }
        } else {
            candidates.insert(key, index);
        }
    }
    let mut all_findings = Vec::new();
    for (index, (mut effects, finding)) in grouped.into_iter().enumerate() {
        if merged.contains(&index) {
            continue;
        }
        effects.sort_by(|a, b| a.region.values.cmp(&b.region.values));
        let identities: BTreeSet<_> = effects
            .iter()
            .flat_map(|effect| [&effect.before_result, &effect.after_result])
            .flatten()
            .map(|result| result.identity.clone())
            .collect();
        let complete: Vec<_> = identities
            .iter()
            .flat_map(|identity| {
                choice_regions(&before_flow, identity)
                    .into_iter()
                    .chain(choice_regions(&after_flow, identity))
            })
            .collect();
        let context = common_context(&complete);
        let choices = identities
            .iter()
            .filter_map(|identity| {
                result_choices
                    .get(identity)
                    .map(|result| FunctionChoiceConditions {
                        result: result.clone(),
                        before: relative(choice_regions(&before_flow, identity), &context),
                        after: relative(choice_regions(&after_flow, identity), &context),
                    })
            })
            .collect();
        let effect = relative(
            effects.iter().map(|effect| effect.region.clone()).collect(),
            &context,
        );
        for item in &mut effects {
            for slot in context.values.keys() {
                item.region.values.remove(slot);
            }
        }
        all_findings.push(CompactResultFinding {
            context,
            choices,
            effect,
            evidence: effects
                .iter()
                .map(|effect| effect.evidence.clone())
                .collect(),
            effects,
            assessment: finding.assessment,
            control_changed: finding.control_changed,
            value_dependency_changed: finding.value_dependency_changed,
            return_structure_changed: finding.return_structure_changed,
            attribution_certain: finding.attribution_certain,
            cause_refs: finding
                .cause_refs
                .iter()
                .map(|index| reference(&id, "causes", *index))
                .collect(),
        });
    }
    let all_count = all_findings.len();
    all_findings.truncate(options.max_groups);
    let omitted_groups = all_count - all_findings.len();
    let equal_regions = comparison
        .regions
        .iter()
        .enumerate()
        .filter(|(_, region)| region.assessment == ResultAssessment::Equal)
        .map(|(index, _)| compact_region(full, &id, index))
        .collect();
    let unknown_regions = comparison
        .regions
        .iter()
        .enumerate()
        .filter(|(_, region)| region.assessment == ResultAssessment::Unknown)
        .map(|(index, _)| compact_region(full, &id, index))
        .collect();
    let unchanged_choices = if comparison.comparison_coverage == Coverage::Complete {
        result_choices
            .into_iter()
            .filter(|(identity, _)| {
                !affected_choices.contains(identity)
                    && choice_regions(&before_flow, identity)
                        == choice_regions(&after_flow, identity)
            })
            .map(|(_, result)| result)
            .collect()
    } else {
        Vec::new()
    };
    let analysis_limits_hit = full
        .analysis
        .before
        .iter()
        .chain(full.analysis.after.iter())
        .flat_map(|side| side.limits_hit.iter().cloned())
        .collect();
    let caller_continuation_open = full
        .analysis
        .before
        .iter()
        .chain(full.analysis.after.iter())
        .flat_map(|side| &side.observations)
        .any(|observation| observation.caller_continuation_open);
    FunctionResultCompactReport {
        schema_version: FUNCTION_RESULT_COMPACT_SCHEMA_VERSION,
        full_report_id: id.clone(),
        evidence_file: format!("{id}.json"),
        snapshots: BTreeSet::from([
            full.analysis.query.before.id.clone(),
            full.analysis.query.after.id.clone(),
        ]),
        selected_function: full.analysis.query.selected_function.clone(),
        counterpart: if full.analysis.query.selected_function.snapshot.side == SnapshotSide::Before
        {
            full.analysis.after.as_ref()
        } else {
            full.analysis.before.as_ref()
        }
        .map(|side| side.function.clone()),
        input_names: full
            .analysis
            .before
            .as_ref()
            .or(full.analysis.after.as_ref())
            .map_or_else(Vec::new, |side| side.parameter_names.clone()),
        before_entry: comparison.before_entry.clone(),
        after_entry: comparison.after_entry.clone(),
        function_presence: comparison.function_presence,
        common_domain_status: comparison.common_domain_status,
        common_domain: comparison.common_domain.clone(),
        before_only_domain: comparison.before_only_domain.clone(),
        after_only_domain: comparison.after_only_domain.clone(),
        unresolved_domain: comparison.unresolved_domain.clone(),
        before_flow,
        after_flow,
        findings: all_findings,
        equal_regions,
        unknown_regions,
        unchanged_choices,
        coverage: FunctionReportCoverage {
            before_analysis: full.analysis.before.as_ref().map(|side| side.coverage),
            after_analysis: full.analysis.after.as_ref().map(|side| side.coverage),
            source_alignment: comparison.source_alignment_coverage,
            input_mapping: comparison.input_mapping_coverage,
            result_comparison: comparison.comparison_coverage,
            presentation: if omitted_groups == 0 {
                Coverage::Complete
            } else {
                Coverage::Partial
            },
            analysis_limits_hit,
            comparison_limits_hit: comparison.limits_hit.clone(),
            omitted_groups,
            caller_continuation_open,
        },
    }
}

pub fn analyze_function_result_reports<P: SnapshotProvider>(
    query: FunctionResultQuery,
    provider: &P,
    before_environment: &AnalysisEnvironment,
    after_environment: &AnalysisEnvironment,
) -> Result<(FunctionResultReport, FunctionResultCompactReport), FunctionResultError> {
    let analysis = analyze_function_result(query, provider, before_environment, after_environment)?;
    let full = assemble_function_result_report(analysis);
    let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
    Ok((full, compact))
}

fn region_text(region: &BooleanRegion, names: &[String]) -> String {
    if region.values.is_empty() {
        return "always".into();
    }
    region
        .values
        .iter()
        .map(|(slot, value)| {
            let name = names
                .get(*slot as usize)
                .cloned()
                .unwrap_or_else(|| format!("arg{slot}"));
            if *value { name } else { format!("!{name}") }
        })
        .collect::<Vec<_>>()
        .join(" && ")
}

fn regions_text(regions: &[BooleanRegion], names: &[String]) -> String {
    if regions.is_empty() {
        return "never".into();
    }
    regions
        .iter()
        .map(|r| {
            let text = region_text(r, names);
            if r.values.len() > 1 && regions.len() > 1 {
                format!("({text})")
            } else {
                text
            }
        })
        .collect::<Vec<_>>()
        .join(" || ")
}

fn truthiness_text(region: &BooleanRegion, names: &[String]) -> String {
    if region.values.is_empty() {
        return "always".into();
    }
    region
        .values
        .iter()
        .map(|(slot, value)| {
            let name = names
                .get(*slot as usize)
                .cloned()
                .unwrap_or_else(|| format!("arg{slot}"));
            format!("{name} is {}", if *value { "truthy" } else { "falsy" })
        })
        .collect::<Vec<_>>()
        .join(" && ")
}

fn assumption_text(expr: &AssumptionExpr, names: &[String]) -> String {
    match expr {
        AssumptionExpr::Input { index } => names
            .get(*index as usize)
            .cloned()
            .unwrap_or_else(|| format!("arg{index}")),
        AssumptionExpr::Literal { value } => match value {
            FunctionKnownValue::Boolean { value } => value.to_string(),
            FunctionKnownValue::Number { value } | FunctionKnownValue::String { value } => {
                value.clone()
            }
            FunctionKnownValue::Null => "null".into(),
            FunctionKnownValue::Undefined => "undefined".into(),
        },
        AssumptionExpr::Unary { operator, operand } => {
            let symbol = match operator {
                AssumptionUnary::Not => "!",
                AssumptionUnary::Plus => "+",
                AssumptionUnary::Minus => "-",
            };
            format!("{symbol}{}", assumption_text(operand, names))
        }
        AssumptionExpr::Binary {
            operator,
            left,
            right,
        } => {
            let symbol = match operator {
                AssumptionBinary::And => "&&",
                AssumptionBinary::Or => "||",
                AssumptionBinary::Nullish => "??",
                AssumptionBinary::Add => "+",
                AssumptionBinary::Subtract => "-",
                AssumptionBinary::Multiply => "*",
                AssumptionBinary::Divide => "/",
                AssumptionBinary::Remainder => "%",
                AssumptionBinary::StrictEqual => "===",
                AssumptionBinary::StrictNotEqual => "!==",
                AssumptionBinary::GreaterThan => ">",
                AssumptionBinary::LessThan => "<",
            };
            format!(
                "({} {symbol} {})",
                assumption_text(left, names),
                assumption_text(right, names)
            )
        }
        AssumptionExpr::Conditional {
            condition,
            when_true,
            when_false,
        } => format!(
            "({} ? {} : {})",
            assumption_text(condition, names),
            assumption_text(when_true, names),
            assumption_text(when_false, names)
        ),
        AssumptionExpr::Unsupported { source, .. } => source.clone(),
    }
}

fn entry_text(entry: &FunctionEntry, names: &[String]) -> String {
    let mut parts = Vec::new();
    for (slot, domain) in &entry.domains {
        let name = names
            .get(*slot as usize)
            .cloned()
            .unwrap_or_else(|| format!("arg{slot}"));
        parts.push(format!("{name}: {domain:?}"));
    }
    for (slot, value) in &entry.known_values {
        let name = names
            .get(*slot as usize)
            .cloned()
            .unwrap_or_else(|| format!("arg{slot}"));
        parts.push(format!("{name} = {value:?}"));
    }
    parts.extend(
        entry
            .assumptions
            .iter()
            .map(|expr| assumption_text(expr, names)),
    );
    if parts.is_empty() {
        "all inputs".into()
    } else {
        parts.join("; ")
    }
}

impl FunctionResultCompactReport {
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        let report: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if report.schema_version != FUNCTION_RESULT_COMPACT_SCHEMA_VERSION {
            return Err(format!(
                "unsupported compact function result version {}",
                report.schema_version
            ));
        }
        Ok(report)
    }

    pub fn validate_with_full(&self, full: &FunctionResultReport) -> Result<(), String> {
        full.validate_control_causes()?;
        if self.schema_version != FUNCTION_RESULT_COMPACT_SCHEMA_VERSION
            || full.schema_version != FUNCTION_RESULT_REPORT_SCHEMA_VERSION
            || self.full_report_id != report_id(full)
            || self.snapshots
                != BTreeSet::from([
                    full.analysis.query.before.id.clone(),
                    full.analysis.query.after.id.clone(),
                ])
        {
            return Err("compact function report does not match full evidence".into());
        }
        for evidence in self
            .findings
            .iter()
            .flat_map(|finding| finding.evidence.iter())
            .chain(self.equal_regions.iter().map(|region| &region.evidence))
            .chain(self.unknown_regions.iter().map(|region| &region.evidence))
        {
            if evidence.report_id != self.full_report_id
                || evidence.section != "comparison.regions"
                || evidence.index >= full.comparison.regions.len()
            {
                return Err(format!("invalid function evidence reference: {evidence:?}"));
            }
        }
        for evidence in self.findings.iter().flat_map(|finding| &finding.cause_refs) {
            if evidence.report_id != self.full_report_id
                || evidence.section != "causes"
                || evidence.index >= full.causes.len()
            {
                return Err(format!("invalid function cause reference: {evidence:?}"));
            }
        }
        for (section, flows, side) in [
            (
                "analysis.before.observations",
                &self.before_flow,
                &full.analysis.before,
            ),
            (
                "analysis.after.observations",
                &self.after_flow,
                &full.analysis.after,
            ),
        ] {
            for flow in flows {
                for evidence in &flow.evidence {
                    if evidence.report_id != self.full_report_id
                        || evidence.section != section
                        || evidence.index >= side.as_ref().map_or(0, |side| side.observations.len())
                    {
                        return Err(format!("invalid function flow reference: {evidence:?}"));
                    }
                }
            }
        }
        let mut actual = self.clone();
        actual.evidence_file = format!("{}.json", self.full_report_id);
        let expected = compact_function_result_report(
            full,
            FunctionPresentationOptions {
                max_groups: self.findings.len(),
            },
        );
        if actual != expected {
            return Err("compact function facts do not match full evidence".into());
        }
        Ok(())
    }

    pub fn render_text(&self, full: &FunctionResultReport) -> String {
        self.render_text_with_detail(full, false)
    }

    /// The detailed presentation retained from the 003 renderer.
    pub fn render_verbose_text(&self, full: &FunctionResultReport) -> String {
        self.render_text_with_detail(full, true)
    }

    fn render_text_with_detail(&self, full: &FunctionResultReport, verbose: bool) -> String {
        debug_assert!(self.validate_with_full(full).is_ok());
        let names = &self.input_names;
        let selection_rule = changed_return_selection(self, full);
        let control_pair = selection_rule
            .as_ref()
            .map(|rule| rule.pair.clone())
            .or_else(|| changed_guard_pair(full));
        let mut lines = vec![format!(
            "Function result: {}",
            self.selected_function
                .expected_name
                .as_deref()
                .unwrap_or("<anonymous>")
        )];
        if self.before_entry != self.after_entry {
            lines.push("Entry assumptions changed (query scope).".into());
            lines.push(format!(
                "Before scope: {}",
                entry_text(&self.before_entry, names)
            ));
            lines.push(format!(
                "After scope: {}",
                entry_text(&self.after_entry, names)
            ));
            lines.push(format!(
                "Common inputs: {}",
                regions_text(&self.common_domain, names)
            ));
            lines.push(format!(
                "Before only: {}",
                regions_text(&self.before_only_domain, names)
            ));
            lines.push(format!(
                "After only: {}",
                regions_text(&self.after_only_domain, names)
            ));
        } else if self.before_entry != FunctionEntry::default() {
            lines.push(format!(
                "Entry scope: {}",
                entry_text(&self.before_entry, names)
            ));
        }
        if self.function_presence != FunctionPresence::Both {
            let status = match self.function_presence {
                FunctionPresence::BeforeOnly => "Function removed (confirmed absent after).",
                FunctionPresence::AfterOnly => "Function added (confirmed absent before).",
                FunctionPresence::Unresolved => "Function counterpart unresolved.",
                FunctionPresence::Both => unreachable!(),
            };
            lines.push(status.into());
            for flow in &self.before_flow {
                lines.push(format!(
                    "Before {}: {} ({:?})",
                    region_text(&flow.flow.region, names),
                    flow.result
                        .as_ref()
                        .map_or_else(|| "unknown".into(), PresentedResult::display),
                    flow.flow.completion
                ));
            }
            for flow in &self.after_flow {
                lines.push(format!(
                    "After {}: {} ({:?})",
                    region_text(&flow.flow.region, names),
                    flow.result
                        .as_ref()
                        .map_or_else(|| "unknown".into(), PresentedResult::display),
                    flow.flow.completion
                ));
            }
        } else if self.common_domain_status == CommonDomainStatus::Empty {
            lines.push("Before flow:".into());
            for flow in &self.before_flow {
                lines.push(format!(
                    "  {}: {} ({:?})",
                    region_text(&flow.flow.region, names),
                    flow.result
                        .as_ref()
                        .map_or_else(|| "unknown".into(), PresentedResult::display),
                    flow.flow.completion
                ));
            }
            lines.push("After flow:".into());
            for flow in &self.after_flow {
                lines.push(format!(
                    "  {}: {} ({:?})",
                    region_text(&flow.flow.region, names),
                    flow.result
                        .as_ref()
                        .map_or_else(|| "unknown".into(), PresentedResult::display),
                    flow.flow.completion
                ));
            }
            lines.push("No common inputs; no cross-version result relation.".into());
        } else {
            if self.common_domain_status == CommonDomainStatus::Unresolved {
                lines.push("Common input feasibility is unresolved.".into());
            }
            lines.extend(dependency_edit_lines(self, full));
            lines.extend(control_cause_lines(self, full));
            let mut shown_controls = BTreeSet::new();
            if let Some(rule) = &selection_rule {
                shown_controls.insert(rule.lines[0].clone());
                lines.extend(rule.lines.clone());
            }
            let (shared_lines, shared_regions) = if verbose {
                (Vec::new(), BTreeSet::new())
            } else {
                shared_computation_lines(self, full)
            };
            lines.extend(shared_lines);
            let (unchanged_lines, summary_regions) = unchanged_data_flow_summary(self, full);
            let unchanged_regions = if verbose {
                BTreeSet::new()
            } else {
                summary_regions.clone()
            };
            lines.extend(equal_control_lines(
                self,
                full,
                control_pair.as_ref(),
                verbose,
                &mut shown_controls,
                &unchanged_regions,
            ));
            if verbose {
                // Equal regions without a control edit are otherwise omitted
                // from the detailed effect loop. Keep the summary's individual
                // computations available in verbose text as well.
                for index in summary_regions {
                    let region = &full.comparison.regions[index];
                    if region.assessment == ResultAssessment::Equal
                        && !region.control_changed
                        && let Some(line) = unchanged_data_flow_line(full, region, names)
                    {
                        lines.push(line);
                    }
                }
            } else {
                lines.extend(unchanged_lines);
            }
            if normal_results_unchanged(self, full) {
                lines.push(format!(
                    "Normal results unchanged on common inputs: {} (under the recorded entry assumptions).",
                    regions_text(&self.common_domain, names)
                ));
            }
            for finding in &self.findings {
                if finding.assessment == ResultAssessment::Equal {
                    continue;
                }
                if finding
                    .effects
                    .iter()
                    .all(|effect| shared_regions.contains(&effect.evidence.index))
                {
                    continue;
                }
                let selected_values = finding.effects.iter().all(|effect| {
                    selected_operand_text(&effect.before_result, &full.analysis.before).is_some()
                        && selected_operand_text(&effect.after_result, &full.analysis.after)
                            .is_some()
                });
                let computations: Vec<_> = finding
                    .effects
                    .iter()
                    .map(|effect| {
                        if effect.assessment == ResultAssessment::Changed
                            && effect
                                .before_result
                                .as_ref()
                                .map(|result| &result.expression)
                                == effect
                                    .after_result
                                    .as_ref()
                                    .map(|result| &result.expression)
                        {
                            changed_computation_lines(
                                full,
                                &full.comparison.regions[effect.evidence.index],
                                names,
                            )
                        } else {
                            Vec::new()
                        }
                    })
                    .collect();
                let show_choices = !selected_values
                    && (verbose
                        || (conditional_choices(finding)
                            && computations.iter().any(Vec::is_empty)));
                let context = region_text(&finding.context, names);
                if !selected_values
                    && (verbose || show_choices)
                    && !finding.context.values.is_empty()
                {
                    lines.push(format!("Context: {context}"));
                }
                if !finding.attribution_certain {
                    lines.push("Edit attribution uncertain".into());
                }
                if !verbose && finding.control_changed {
                    let control = control_text(
                        full,
                        finding.effects.iter().map(|effect| effect.evidence.index),
                        control_pair.as_ref(),
                    );
                    if shown_controls.insert(control.clone()) {
                        lines.push(control);
                    }
                }
                if show_choices {
                    lines.push("Return choice | Before | After".into());
                    for choice in &finding.choices {
                        lines.push(format!(
                            "{} | {} | {}",
                            choice.result.display(),
                            regions_text(&choice.before, names),
                            regions_text(&choice.after, names)
                        ));
                    }
                }
                for (effect, computation) in finding.effects.iter().zip(computations) {
                    if !computation.is_empty() {
                        lines.extend(computation);
                    } else if selected_values {
                        let mut condition = effect.region.clone();
                        condition.values.extend(&finding.context.values);
                        lines.push(format!(
                            "when {}: {} -> {} ({:?})",
                            truthiness_text(&condition, names),
                            selected_operand_text(&effect.before_result, &full.analysis.before)
                                .unwrap(),
                            selected_operand_text(&effect.after_result, &full.analysis.after)
                                .unwrap(),
                            effect.assessment
                        ));
                    } else {
                        let mut condition = effect.region.clone();
                        if !verbose && !show_choices {
                            condition.values.extend(&finding.context.values);
                        }
                        lines.push(format!(
                            "Effect: {}: {} -> {} ({:?})",
                            region_text(&condition, names),
                            effect
                                .before_result
                                .as_ref()
                                .map_or_else(|| "unknown".into(), PresentedResult::display),
                            effect
                                .after_result
                                .as_ref()
                                .map_or_else(|| "unknown".into(), PresentedResult::display),
                            effect.assessment
                        ));
                    }
                    if verbose {
                        for result in [&effect.before_result, &effect.after_result]
                            .into_iter()
                            .flatten()
                        {
                            lines.extend(source_text(result));
                        }
                    }
                }
                if !verbose {
                    lines.extend(key_source_text(finding, full));
                }
            }
            let mut marked_unknown_findings = BTreeSet::new();
            for unknown in &self.unknown_regions {
                let unchanged = unchanged_data_flow_line(
                    full,
                    &full.comparison.regions[unknown.evidence.index],
                    names,
                );
                for (index, finding) in full.findings.iter().enumerate() {
                    if finding.assessment == ResultAssessment::Unknown
                        && finding.regions.contains(&unknown.evidence.index)
                        && marked_unknown_findings.insert(index)
                    {
                        if !finding.attribution_certain
                            && finding.regions.iter().any(|index| {
                                unchanged_data_flow_line(
                                    full,
                                    &full.comparison.regions[*index],
                                    names,
                                )
                                .is_none()
                            })
                        {
                            lines.push("Edit attribution uncertain".into());
                        }
                        if !verbose && finding.control_changed {
                            let control = control_text(
                                full,
                                finding.regions.iter().copied(),
                                control_pair.as_ref(),
                            );
                            if shown_controls.insert(control.clone()) {
                                lines.push(control);
                            }
                        }
                    }
                }
                if unchanged_regions.contains(&unknown.evidence.index) {
                    continue;
                }
                if let Some(line) = unchanged {
                    lines.push(line);
                    continue;
                }
                lines.push(format!(
                    "Unknown under {}: {}",
                    region_text(&unknown.region, names),
                    unknown.reason.as_deref().unwrap_or("comparison unresolved")
                ));
            }
        }
        lines.push(coverage_text(&self.coverage));
        lines.push(format!(
            "Evidence: {} ({})",
            self.evidence_file, self.full_report_id
        ));
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ifds::adapters::typescript::{index_bindings, index_functions};
    use crate::ifds::model::{
        AnalysisLimits, CapabilitySet, FileChange, FileChangeKind, RepositoryDiff, SnapshotHandle,
        SnapshotSide,
    };
    use crate::ifds::snapshots::{InMemorySnapshot, InMemorySnapshotProvider};

    fn analyze(
        before: &str,
        after: &str,
        before_entry: FunctionEntry,
        after_entry: FunctionEntry,
    ) -> FunctionResultAnalysis {
        let path = "src/fixture.ts";
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
        analyze_function_result(query, &provider, &environment, &environment).unwrap()
    }

    fn enabled() -> FunctionResultAnalysis {
        analyze(
            "function result(enabled: boolean, ready: boolean) { if (enabled) return \"ok\"; return \"skip\"; }",
            "function result(enabled: boolean, ready: boolean) { if (enabled && ready) return \"ok\"; return \"skip\"; }",
            FunctionEntry::default(),
            FunctionEntry::default(),
        )
    }

    fn review(name: &str) -> (FunctionResultReport, FunctionResultCompactReport) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("spec/function-result-review/reports")
            .join(name);
        let before = std::fs::read_to_string(root.join("before.ts")).unwrap();
        let after = std::fs::read_to_string(root.join("after.ts")).unwrap();
        let full = assemble_function_result_report(analyze(
            &before,
            &after,
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        compact.validate_with_full(&full).unwrap();
        (full, compact)
    }

    #[test]
    fn changed_guard_input_shows_return_source_selection_before_pairwise_effects() {
        let before = include_str!(
            "../../spec/function-result-review/complex-batch-2026-10-03/03-guard-version/before.ts"
        );
        let after = include_str!(
            "../../spec/function-result-review/complex-batch-2026-10-03/03-guard-version/after.ts"
        );
        let full = assemble_function_result_report(analyze(
            before,
            after,
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let full_bytes = serde_json::to_vec(&full).unwrap();
        let compact_bytes = serde_json::to_vec(&compact).unwrap();
        for verbose in [false, true] {
            let text = compact.render_text_with_detail(&full, verbose);
            let selection = "Control: !first -> second\nReturn source: b - a\n  Before: under !first\n  After: under second";
            assert!(text.contains(selection), "{text}");
            assert_eq!(
                text.matches("Control: !first -> second").count(),
                1,
                "{text}"
            );
            assert!(!text.contains("correspondence unresolved"), "{text}");
            let effects = text.find("Changed computation under").unwrap();
            assert!(text.find(selection).unwrap() < effects, "{text}");
            assert!(
                text.contains(
                    "Changed computation under first && second:\n  (a + b) -> (b - a) (Changed)"
                ),
                "{text}"
            );
            assert!(
                text.contains(
                    "Changed computation under !first && !second:\n  (b - a) -> a (Changed)"
                ),
                "{text}"
            );
        }
        for (first, second) in [(false, false), (false, true), (true, false), (true, true)] {
            let region = full
                .comparison
                .regions
                .iter()
                .find(|region| region.region.values == BTreeMap::from([(2, first), (3, second)]))
                .unwrap();
            assert_eq!(region.value_dependency_changed, first == second);
        }
        assert_eq!(serde_json::to_vec(&full).unwrap(), full_bytes);
        assert_eq!(serde_json::to_vec(&compact).unwrap(), compact_bytes);
        compact.validate_with_full(&full).unwrap();
    }

    #[test]
    fn changed_return_selection_retains_the_enclosing_reaching_condition() {
        let before = include_str!(
            "../../spec/function-result-review/complex-batch-2026-10-03/03-guard-version/before.ts"
        )
        .replace("first: boolean", "enabled: boolean, first: boolean")
        .replace(
            "  let decision",
            "  if (!enabled) return 0;\n  let decision",
        );
        let after = before.replace("decision = !decision;", "decision = second;");
        let full = assemble_function_result_report(analyze(
            &before,
            &after,
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        assert!(text.contains("Return source: b - a\n  Before: under enabled && !first\n  After: under enabled && second"), "{text}");
        assert!(!text.contains("  After: under second\n"), "{text}");
        compact.validate_with_full(&full).unwrap();
    }

    #[test]
    fn control_cause_follows_the_assignment_and_links_all_affected_regions() {
        let before = include_str!(
            "../../spec/function-result-review/complex-batch-2026-10-03/03-guard-version/before.ts"
        );
        let after = before.replace("decision = !decision;", "decision = second;");
        let full = assemble_function_result_report(analyze(
            before,
            &after,
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        assert_eq!(full.causes.len(), 1);
        let cause = &full.causes[0];
        assert_eq!(cause.kind, FunctionResultCauseKind::ControlSourceChanged);
        assert_eq!(cause.before.expression, "(!first)");
        assert_eq!(cause.after.expression, "second");
        for (side, evidence) in [
            (full.analysis.before.as_ref().unwrap(), &cause.before),
            (full.analysis.after.as_ref().unwrap(), &cause.after),
        ] {
            assert_eq!(evidence.edit.operation, "write");
            assert_eq!(evidence.edit.span.as_ref().unwrap().start_line, 4);
            assert!(!evidence.edit.path.is_empty());
            let guard = &side.observations[evidence.observation_index].guards[evidence.guard_index];
            assert_eq!(guard.branch, evidence.branch);
            let mut dependency = &guard.condition;
            for index in &evidence.edit.path {
                dependency = &dependency.inputs[*index];
            }
            assert_eq!(dependency.node, evidence.edit.node);
            assert_eq!(dependency.span, evidence.edit.span);
        }
        assert_ne!(
            cause.before.edit.node.snapshot,
            cause.after.edit.node.snapshot
        );
        let changed_regions = cause
            .regions
            .iter()
            .filter(|index| full.comparison.regions[**index].value_dependency_changed)
            .map(|index| full.comparison.regions[*index].region.values.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            changed_regions,
            BTreeSet::from([
                BTreeMap::from([(2, true), (3, true)]),
                BTreeMap::from([(2, false), (3, false)])
            ])
        );
        assert!(
            full.findings
                .iter()
                .all(|finding| finding.attribution_certain && finding.cause_refs == vec![0])
        );
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        for verbose in [false, true] {
            let text = compact.render_text_with_detail(&full, verbose);
            assert_eq!(
                text.matches(
                    "Edit: (!first) -> second (before src/fixture.ts:4; after src/fixture.ts:4)"
                )
                .count(),
                1,
                "{text}"
            );
            assert!(!text.contains("Edit attribution uncertain"), "{text}");
        }
        let restored =
            FunctionResultReport::from_json(&serde_json::to_vec(&full).unwrap()).unwrap();
        assert_eq!(restored, full);
        FunctionResultCompactReport::from_json(&serde_json::to_vec(&compact).unwrap())
            .unwrap()
            .validate_with_full(&restored)
            .unwrap();
        for mutation in 0..3 {
            let mut corrupted = full.clone();
            match mutation {
                0 => corrupted.causes[0].before.edit.path.push(usize::MAX),
                1 => corrupted.causes[0].regions.clear(),
                _ => corrupted.findings[0].cause_refs.clear(),
            }
            assert!(
                FunctionResultReport::from_json(&serde_json::to_vec(&corrupted).unwrap()).is_err()
            );
            assert!(compact.validate_with_full(&corrupted).is_err());
        }
        let mut corrupted = compact.clone();
        corrupted.findings[0].cause_refs[0].index = usize::MAX;
        assert!(corrupted.validate_with_full(&full).is_err());
        let hidden =
            compact_function_result_report(&full, FunctionPresentationOptions { max_groups: 0 });
        assert!(!hidden.render_text(&full).contains("Edit:"));
    }

    #[test]
    fn control_cause_traces_copies_and_retains_the_captured_variable_version() {
        let cases = [
            (
                "function result(first, second) {\nlet decision = first;\nconst copied = decision;\nconst alias = copied;\nif (alias) return 1;\nreturn 2;\n}",
                "let decision = first;",
                "let decision = second;",
            ),
            (
                "function result(first, second) {\nlet decision = first;\nconst saved = decision;\ndecision = second;\nif (saved) return 1;\nreturn 2;\n}",
                "let decision = first;",
                "let decision = !first;",
            ),
        ];
        for (before, old, new) in cases {
            let after = before.replace(old, new).replace(
                "decision = second;\nif (saved)",
                "decision = first;\nif (saved)",
            );
            let full = assemble_function_result_report(analyze(
                before,
                &after,
                FunctionEntry::default(),
                FunctionEntry::default(),
            ));
            assert_eq!(full.causes.len(), 1, "{full:#?}");
            let cause = &full.causes[0];
            assert_eq!(cause.before.edit.span.as_ref().unwrap().start_line, 2);
            assert_eq!(cause.after.edit.span.as_ref().unwrap().start_line, 2);
            assert!(cause.before.edit.path.len() > 3);
            assert!(
                full.findings
                    .iter()
                    .all(|finding| finding.attribution_certain)
            );
            let compact =
                compact_function_result_report(&full, FunctionPresentationOptions::default());
            assert!(
                !compact
                    .render_text(&full)
                    .contains("Edit attribution uncertain")
            );
        }
    }

    #[test]
    fn control_cause_rejects_multiple_edits_unknown_paths_and_killed_changes() {
        let base = include_str!(
            "../../spec/function-result-review/complex-batch-2026-10-03/03-guard-version/before.ts"
        );
        let changed = base.replace("decision = !decision;", "decision = second;");
        let same_arms = "function result(first, second) {\nlet decision = first;\nif (decision) return 1;\nreturn 1;\n}";
        for (before, after) in [
            (base.to_string(), changed.replace("value = b - a;", "value = b + a;")),
            (base.to_string(), changed.replace("const original = decision;", "const original = !decision;")),
            (base.replace("  let value", "  mystery();\n  let value"), changed.replace("  let value", "  mystery();\n  let value")),
            (base.to_string(), base.replace("decision = !decision;", "decision = second;\n  decision = !first;")),
            (same_arms.to_string(), same_arms.replace("decision = first;", "decision = second;")),
            ("function result(first, second) {\nlet decision = first;\nconst saved = decision;\ndecision = first;\nif (saved) return 1;\nreturn 2;\n}".into(), "function result(first, second) {\nlet decision = first;\nconst saved = decision;\ndecision = second;\nif (saved) return 1;\nreturn 2;\n}".into()),
            ("function result(first, second) {\nlet decision = first;\nlet selector = decision;\nif (selector) return 1;\nreturn 2;\n}".into(), "function result(first, second) {\nlet decision = second;\nlet selector = !decision;\nif (selector) return 1;\nreturn 2;\n}".into()),
            ("function result(first, second) {\nlet decision = first;\nif (decision) return 1;\nreturn 2;\n}".into(), "function result(first, second) {\nlet decision = second;\nif (!decision) return 1;\nreturn 2;\n}".into()),
            (base.replace('\n', " "), changed.replace('\n', " ")),
        ] {
            let full = assemble_function_result_report(analyze(&before, &after, FunctionEntry::default(), FunctionEntry::default()));
            assert!(full.causes.is_empty(), "{before}\n{after}\n{:#?}", full.causes);
        }
        let analysis = analyze(
            base,
            &changed,
            FunctionEntry::default(),
            FunctionEntry::default(),
        );
        let limited = assemble_function_result_report_with_comparison_limit(analysis.clone(), 1);
        assert!(limited.causes.is_empty());
        for mutation in 0..3 {
            let mut incomplete = analysis.clone();
            match mutation {
                0 => incomplete.input_mapping_coverage = Coverage::Partial,
                1 => incomplete.input_correspondence.clear(),
                _ => incomplete.after.as_mut().unwrap().coverage = Coverage::Partial,
            }
            assert!(
                assemble_function_result_report(incomplete)
                    .causes
                    .is_empty()
            );
        }
    }

    #[test]
    fn return_selection_explanation_rejects_ambiguous_or_unresolved_changes() {
        let before = include_str!(
            "../../spec/function-result-review/complex-batch-2026-10-03/03-guard-version/before.ts"
        );
        let after = before.replace("decision = !decision;", "decision = second;");
        for (old, new) in [
            (
                before.to_string(),
                after.replace("value = b - a;", "value = b + a;"),
            ),
            (
                before.to_string(),
                after.replace(
                    "if (decision) value = b - a;",
                    "if (decision) {\n    if (first) value = b - a;\n  }",
                ),
            ),
            (
                before.to_string(),
                after.replace("  return value;", "  value = a;\n  return value;"),
            ),
            (
                before.replace("  let value", "  mystery();\n  let value"),
                after.replace("  let value", "  mystery();\n  let value"),
            ),
            (before.replace('\n', " "), after.replace('\n', " ")),
            (
                "function result(first, second) { if (first) return 1; return 1; }".into(),
                "function result(first, second) { if (second) return 1; return 1; }".into(),
            ),
        ] {
            let full = assemble_function_result_report(analyze(
                &old,
                &new,
                FunctionEntry::default(),
                FunctionEntry::default(),
            ));
            let compact =
                compact_function_result_report(&full, FunctionPresentationOptions::default());
            let text = compact.render_text(&full);
            assert!(!text.contains("Return source:"), "{old}\n{new}\n{text}");
            compact.validate_with_full(&full).unwrap();
        }
        let full = assemble_function_result_report(analyze(
            before,
            &after,
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        for max_groups in [0, 1] {
            let compact =
                compact_function_result_report(&full, FunctionPresentationOptions { max_groups });
            let text = compact.render_text(&full);
            assert!(!text.contains("Return source:"), "{text}");
            assert!(text.contains("presentation Partial"), "{text}");
            compact.validate_with_full(&full).unwrap();
        }
        let mut analysis = full.analysis.clone();
        analysis.input_mapping_coverage = Coverage::Partial;
        analysis.input_correspondence.clear();
        let full = assemble_function_result_report(analysis);
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        assert!(!compact.render_text(&full).contains("Return source:"));
    }

    #[test]
    fn reordered_overwrites_report_all_four_data_flow_regions() {
        let before = "function choose(a: number, b: number, first: boolean, second: boolean) {\n  let result = a;\n\n  if (first) result = a + b;\n  if (second) result = b * 2;\n\n  return result;\n}";
        let after = "function choose(a: number, b: number, first: boolean, second: boolean) {\n  let result = a;\n\n  if (second) result = b * 2;\n  if (first) result = a + b;\n\n  return result;\n}";
        let full = assemble_function_result_report(analyze(
            before,
            after,
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let bytes = serde_json::to_vec(&full).unwrap();
        for verbose in [false, true] {
            let text = compact.render_text_with_detail(&full, verbose);
            assert_eq!(
                text.matches("Control: guard order first then second -> second then first")
                    .count(),
                1,
                "{text}"
            );
            if verbose {
                assert!(
                    text.contains("Unchanged data flow under !first && !second: a (Equal)"),
                    "{text}"
                );
                assert!(text.contains("Unchanged data flow under !first && second: (b * 2) (runtime value equality unresolved)"), "{text}");
                assert!(text.contains("Unchanged data flow under first && !second: (a + b) (runtime value equality unresolved)"), "{text}");
            } else {
                assert!(text.contains("Unchanged data flow under !first || !second (runtime value equality unresolved in some regions)"), "{text}");
                assert_eq!(text.matches("Unchanged data flow").count(), 1, "{text}");
            }
            assert!(
                text.contains(
                    "Changed computation under first && second:\n  (b * 2) -> (a + b) (Changed)"
                ),
                "{text}"
            );
            assert!(!text.contains("correspondence unresolved"), "{text}");
            assert!(!text.contains("Unknown under"), "{text}");
            assert!(!text.contains("Normal results unchanged"), "{text}");
            if !verbose {
                assert!(!text.contains("Return choice |"), "{text}");
                assert_eq!(
                    text.matches("Edit attribution uncertain").count(),
                    1,
                    "{text}"
                );
            }
        }
        // Presentation cannot upgrade unknown runtime equality to Equal or
        // complete comparison coverage. Only the shared dependencies are known.
        assert_eq!(full.comparison.regions.len(), 4);
        for (first, second, assessment) in [
            (false, false, ResultAssessment::Equal),
            (false, true, ResultAssessment::Unknown),
            (true, false, ResultAssessment::Unknown),
            (true, true, ResultAssessment::Changed),
        ] {
            let region = full
                .comparison
                .regions
                .iter()
                .find(|region| region.region.values == BTreeMap::from([(2, first), (3, second)]))
                .unwrap();
            assert_eq!(region.assessment, assessment);
            assert_eq!(region.value_dependency_changed, first && second);
        }
        assert_eq!(full.comparison.comparison_coverage, Coverage::Partial);
        assert_eq!(serde_json::to_vec(&full).unwrap(), bytes);
        compact.validate_with_full(&full).unwrap();
    }

    #[test]
    fn unchanged_region_union_excludes_unknown_completion_and_omitted_groups() {
        let source =
            "function result(a, b) { if (a) { mystery(); return 0; } if (b) return 1; return 2; }";
        let full = assemble_function_result_report(analyze(
            source,
            source,
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let bytes = serde_json::to_vec(&full).unwrap();
        let text = compact.render_text(&full);
        assert!(
            text.contains("Unchanged data flow under !a (Equal)"),
            "{text}"
        );
        assert!(
            text.contains("Unknown under a: an unresolved call may prevent normal completion"),
            "{text}"
        );
        assert!(!text.contains("Unchanged data flow under always"), "{text}");
        let verbose = compact.render_verbose_text(&full);
        assert!(
            verbose.contains("Unchanged data flow under !a && !b: 2 (Equal)"),
            "{verbose}"
        );
        assert!(
            verbose.contains("Unchanged data flow under !a && b: 1 (Equal)"),
            "{verbose}"
        );
        assert_eq!(serde_json::to_vec(&full).unwrap(), bytes);
        compact.validate_with_full(&full).unwrap();

        let (full, _) = review("priority");
        let compact =
            compact_function_result_report(&full, FunctionPresentationOptions { max_groups: 0 });
        let text = compact.render_text(&full);
        assert!(!text.contains("Unchanged data flow"), "{text}");
        assert!(text.contains("presentation Partial"), "{text}");
        compact.validate_with_full(&full).unwrap();
    }

    #[test]
    fn unchanged_data_flow_does_not_require_runtime_value_equality() {
        let full = assemble_function_result_report(analyze(
            "function result(a, b) { return a + b; }",
            "function result(left, right) { const saved = left + right; return saved; }",
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        assert!(
            text.contains(
                "Unchanged data flow under always: (a + b) (runtime value equality unresolved)"
            ),
            "{text}"
        );
        assert!(!text.contains("Unknown under"), "{text}");
        assert!(!text.contains("(Equal)"), "{text}");
        assert!(!text.contains("Normal results unchanged"), "{text}");
        assert!(!text.contains("Edit attribution uncertain"), "{text}");
        compact.validate_with_full(&full).unwrap();
    }

    #[test]
    fn unresolved_selection_completion_or_pairing_cannot_claim_unchanged_data_flow() {
        for source in [
            "function result(a) { return mystery(a); }",
            "function result(a) { mystery(); return a + 1; }",
            "function result(a) { if (mystery(a)) return a + 1; return a + 2; }",
            "function result(a) { if (a === 1) return a + 1; return a + 2; }",
        ] {
            let full = assemble_function_result_report(analyze(
                source,
                source,
                FunctionEntry::default(),
                FunctionEntry::default(),
            ));
            let compact =
                compact_function_result_report(&full, FunctionPresentationOptions::default());
            let text = compact.render_text(&full);
            assert!(!text.contains("Unchanged data flow"), "{source}: {text}");
            assert!(text.contains("Unknown under"), "{source}: {text}");
            compact.validate_with_full(&full).unwrap();
        }
        let source = "function result(a) { return a + 1; }";
        let mut analysis = analyze(
            source,
            source,
            FunctionEntry::default(),
            FunctionEntry::default(),
        );
        analysis.input_mapping_coverage = Coverage::Partial;
        analysis.input_correspondence.clear();
        let full = assemble_function_result_report(analysis);
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        assert!(!text.contains("Unchanged data flow"), "{text}");
        assert!(text.contains("Unknown under"), "{text}");
    }

    #[test]
    fn unchanged_flow_beside_an_unresolved_call_preserves_the_unknown_region() {
        let source = "function result(flag, input) { if (flag) { mystery(); return input + 1; } return input + 1; }";
        let full = assemble_function_result_report(analyze(
            source,
            source,
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        assert!(
            text.contains(
                "Unchanged data flow under !flag: (input + 1) (runtime value equality unresolved)"
            ),
            "{text}"
        );
        assert!(
            text.contains("Unknown under flag: an unresolved call may prevent normal completion"),
            "{text}"
        );
        assert_eq!(
            text.matches("Edit attribution uncertain").count(),
            1,
            "{text}"
        );
        assert!(!text.contains("Unchanged data flow under flag:"), "{text}");
        compact.validate_with_full(&full).unwrap();
    }

    #[test]
    fn reordered_guard_versions_cannot_be_described_as_a_simple_permutation() {
        let full = assemble_function_result_report(analyze(
            "function result(a, b, first, second) { let flag = first; let value = a; if (flag) value = a + b; if (second) value = b * 2; return value; }",
            "function result(a, b, first, second) { let flag = first; let value = a; if (second) value = b * 2; flag = !flag; if (flag) value = a + b; return value; }",
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        assert!(!text.contains("Control: guard order"), "{text}");
        assert!(
            text.contains("Control changed (guard correspondence unresolved)"),
            "{text}"
        );
        compact.validate_with_full(&full).unwrap();
    }

    #[test]
    fn shared_upstream_edit_is_attributed_across_return_paths() {
        let before = "function calculate(a: number, b: number, flag: boolean) {\n  let current = a + b;\n  const saved = current * 2;\n\n  current = b;\n  if (flag) current = 100;\n\n  return saved + current;\n}";
        let after = before.replace("a + b", "a - b");
        let full = assemble_function_result_report(analyze(
            before,
            &after,
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        assert_eq!(full.findings.len(), 1);
        assert_eq!(full.findings[0].regions.len(), 2);
        assert!(full.findings[0].attribution_certain);
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let full_bytes = serde_json::to_vec(&full).unwrap();
        let compact_bytes = serde_json::to_vec(&compact).unwrap();
        for text in [
            compact.render_text(&full),
            compact.render_verbose_text(&full),
        ] {
            assert_eq!(text.matches("Edit:").count(), 1, "{text}");
            assert!(
                text.contains(
                    "Edit: (a + b) -> (a - b) (before src/fixture.ts:2; after src/fixture.ts:2)"
                ),
                "{text}"
            );
            assert!(!text.contains("Edit attribution uncertain"), "{text}");
        }
        let short = compact.render_text(&full);
        assert_eq!(short.matches("Changed computation").count(), 1, "{short}");
        assert_eq!(short.matches("Reaches return").count(), 1, "{short}");
        assert!(
            short.contains(
                "Changed computation for saved:\n  ((a + b) * 2) -> ((a - b) * 2) (Changed)"
            ),
            "{short}"
        );
        assert!(short.contains("Return under !flag: (saved + b)\n  Context writes: before src/fixture.ts:5; after src/fixture.ts:5"), "{short}");
        assert!(short.contains("Return under flag: (saved + 100)\n  Context writes: before src/fixture.ts:6; after src/fixture.ts:6"), "{short}");
        assert_eq!(
            short
                .matches("Before shared writes: src/fixture.ts:2, src/fixture.ts:3")
                .count(),
            1,
            "{short}"
        );
        let verbose = compact.render_verbose_text(&full);
        assert!(
            verbose.contains("Changed computation under flag"),
            "{verbose}"
        );
        assert!(
            verbose.contains("Changed computation under !flag"),
            "{verbose}"
        );
        assert!(short.len() < verbose.len());
        assert_eq!(serde_json::to_vec(&full).unwrap(), full_bytes);
        assert_eq!(serde_json::to_vec(&compact).unwrap(), compact_bytes);
        compact.validate_with_full(&full).unwrap();
        let truncated =
            compact_function_result_report(&full, FunctionPresentationOptions { max_groups: 0 });
        assert!(!truncated.render_text(&full).contains("Edit:"));
        assert!(
            !truncated
                .render_text(&full)
                .contains("Changed computation for")
        );
        let limited =
            compact_function_result_report(&full, FunctionPresentationOptions { max_groups: 1 });
        assert!(
            !limited
                .render_text(&full)
                .contains("Changed computation for")
        );
    }

    #[test]
    fn shared_computation_grouping_requires_one_unambiguous_return_component() {
        for (before, after) in [
            (
                "function result(a, b, flag) {\nlet saved = a + b;\nif (flag) saved = a * b;\nlet tail = b;\nif (flag) tail = 100;\nreturn saved + tail;\n}",
                "function result(a, b, flag) {\nlet saved = a - b;\nif (flag) saved = a / b;\nlet tail = b;\nif (flag) tail = 100;\nreturn saved + tail;\n}",
            ),
            (
                "function result(a, b, flag) {\nconst saved = a + b;\nlet tail = b;\nif (flag) tail = 100;\nreturn saved * saved + tail;\n}",
                "function result(a, b, flag) {\nconst saved = a - b;\nlet tail = b;\nif (flag) tail = 100;\nreturn saved * saved + tail;\n}",
            ),
        ] {
            let full = assemble_function_result_report(analyze(
                before,
                after,
                FunctionEntry::default(),
                FunctionEntry::default(),
            ));
            let compact =
                compact_function_result_report(&full, FunctionPresentationOptions::default());
            let text = compact.render_text(&full);
            assert!(!text.contains("Changed computation for"), "{text}");
            assert!(text.contains("Changed computation under flag"), "{text}");
            assert!(text.contains("Changed computation under !flag"), "{text}");
            compact.validate_with_full(&full).unwrap();
        }
    }

    #[test]
    fn upstream_attribution_keeps_ambiguous_edits_and_boundaries_uncertain() {
        for (before, after) in [
            (
                "function result(p) {\nconst first = p + 1;\nconst second = first * 2;\nreturn second;\n}",
                "function result(p) {\nconst first = p + 3;\nconst second = first * 4;\nreturn second;\n}",
            ),
            (
                "function result(p) {\nmystery();\nconst saved = p + 1;\nreturn saved;\n}",
                "function result(p) {\nmystery();\nconst saved = p + 2;\nreturn saved;\n}",
            ),
            (
                "function result(p) {\nconst saved = p + 1;\nreturn saved;\n}",
                "function result(p) {\nconst extra = p;\nconst saved = extra + 2;\nreturn saved;\n}",
            ),
            (
                "function result(p, flag) {\nlet saved = p + 1;\nif (flag) saved = p + 2;\nreturn saved;\n}",
                "function result(p, flag) {\nlet saved = p + 3;\nif (flag) saved = p + 4;\nreturn saved;\n}",
            ),
        ] {
            let full = assemble_function_result_report(analyze(
                before,
                after,
                FunctionEntry::default(),
                FunctionEntry::default(),
            ));
            assert!(
                full.findings
                    .iter()
                    .all(|finding| !finding.attribution_certain),
                "{full:#?}"
            );
            let compact =
                compact_function_result_report(&full, FunctionPresentationOptions::default());
            let text = compact.render_text(&full);
            assert!(text.contains("Edit attribution uncertain"), "{text}");
            assert!(!text.contains("Edit:"), "{text}");
        }
    }

    #[test]
    fn upstream_attribution_follows_changed_operand_and_ignores_killed_edits() {
        let before = "function result(a, b) {\nlet current = a + 1;\nconst saved = current * 2;\ncurrent = 100;\nreturn saved;\n}";
        let after = before
            .replace("a + 1", "b + 1")
            .replace("current = 100", "current = 200");
        let full = assemble_function_result_report(analyze(
            before,
            &after,
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        assert!(full.findings[0].attribution_certain, "{full:#?}");
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        assert!(
            text.contains("Edit: a -> b (before src/fixture.ts:2; after src/fixture.ts:2)"),
            "{text}"
        );
        assert!(!text.contains("100 -> 200"), "{text}");
        assert!(!text.contains("Edit attribution uncertain"), "{text}");
    }

    #[test]
    fn changed_result_shows_computation_reaching_saved_return_without_input_types() {
        let before = "function compute(input: number, useSaved: boolean) {\n  let current = input;\n  const saved = current + 1;\n  current = 100;\n  if (useSaved) { return saved; }\n  return current;\n}";
        let after = before.replace("current + 1", "current + 2");
        let full = assemble_function_result_report(analyze(
            before,
            &after,
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let full_bytes = serde_json::to_vec(&full).unwrap();
        let compact_bytes = serde_json::to_vec(&compact).unwrap();
        assert!(compact.unknown_regions.is_empty());
        assert_eq!(compact.equal_regions.len(), 1);
        assert_eq!(compact.findings[0].assessment, ResultAssessment::Changed);
        assert_eq!(compact.coverage.result_comparison, Coverage::Complete);
        for text in [
            compact.render_text(&full),
            compact.render_verbose_text(&full),
        ] {
            assert!(
                text.contains("Changed computation under useSaved:\n  (input + 1) -> (input + 2) (Changed)\n  Reaches return saved"),
                "{text}"
            );
            assert!(!text.contains("Unknown under"), "{text}");
            assert!(
                !text.contains("Changed computation under !useSaved"),
                "{text}"
            );
            assert!(!text.contains("(Different)"), "{text}");
        }
        assert_eq!(full_bytes, serde_json::to_vec(&full).unwrap());
        assert_eq!(compact_bytes, serde_json::to_vec(&compact).unwrap());
        compact.validate_with_full(&full).unwrap();
    }

    #[test]
    fn changed_computation_requires_resolved_changed_return_dependencies() {
        for (before, after, changed) in [
            (
                "function result(input: number) { return input + 1; }",
                "function result(input: number) { return input + 2; }",
                true,
            ),
            (
                "function result(input: number) { return input + 1; }",
                "function result(renamed: number) { const saved = renamed + 1; return saved; }",
                false,
            ),
            (
                "function result(input: number) { let saved = input + 1; saved = 100; return saved; }",
                "function result(input: number) { let saved = input + 2; saved = 100; return saved; }",
                false,
            ),
            (
                "function result(input: number) { mystery(); return input + 1; }",
                "function result(input: number) { mystery(); return input + 2; }",
                false,
            ),
        ] {
            let full = assemble_function_result_report(analyze(
                before,
                after,
                FunctionEntry::default(),
                FunctionEntry::default(),
            ));
            let compact =
                compact_function_result_report(&full, FunctionPresentationOptions::default());
            let text = compact.render_text(&full);
            assert_eq!(text.contains("(Changed)"), changed, "{text}");
        }
    }

    #[test]
    fn ifds_fr004_short_replacements_and_verbose_details() {
        for name in [
            "literal",
            "copy",
            "known_expression",
            "fallthrough",
            "short_circuit",
        ] {
            let (full, compact) = review(name);
            let full_bytes = serde_json::to_vec(&full).unwrap();
            let compact_bytes = serde_json::to_vec(&compact).unwrap();
            let short = compact.render_text(&full);
            let verbose = compact.render_verbose_text(&full);
            assert!(!short.contains("Return choice"), "{name}: {short}");
            assert!(!short.contains(" return "), "{name}: {short}");
            for text in [&short, &verbose] {
                assert_eq!(text.matches("Coverage:").count(), 1, "{text}");
                assert_eq!(text.matches("Evidence:").count(), 1, "{text}");
            }
            if name != "short_circuit" {
                assert!(verbose.contains("Return choice"), "{name}: {verbose}");
                assert!(short.len() < verbose.len(), "{name}: {short}");
            }
            if name == "copy" {
                assert!(
                    short.contains("saved (1) -> saved (2) (Different)"),
                    "{short}"
                );
                assert!(
                    short.contains("Before contributing writes: src/fixture.ts:2"),
                    "{short}"
                );
                assert!(
                    short.contains("After contributing writes: src/fixture.ts:2"),
                    "{short}"
                );
                assert!(!short.contains("src/fixture.ts:3"), "{short}");
                assert!(!short.contains("src/fixture.ts:4"), "{short}");
                assert!(
                    verbose.contains("src/fixture.ts:3, src/fixture.ts:2"),
                    "{verbose}"
                );
            }
            assert_eq!(full_bytes, serde_json::to_vec(&full).unwrap());
            assert_eq!(compact_bytes, serde_json::to_vec(&compact).unwrap());
            compact.validate_with_full(&full).unwrap();
        }
    }

    #[test]
    fn ifds_fr004_keeps_computations_and_multiple_value_origins() {
        let before = "function result() {\nlet a = 1;\nlet b = 2;\nlet computed = a + b;\nconst saved = computed;\na = 99;\nreturn saved;\n}";
        let after = before.replace("a = 1", "a = 3");
        let full = assemble_function_result_report(analyze(
            before,
            &after,
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        for side in ["Before", "After"] {
            assert!(text.contains(&format!("{side} contributing writes: src/fixture.ts:2, src/fixture.ts:3, src/fixture.ts:4")), "{text}");
        }
        assert!(!text.contains("src/fixture.ts:5"), "{text}");
        assert!(!text.contains("src/fixture.ts:6"), "{text}");
        assert!(!text.contains("Return choice"), "{text}");
        assert!(text.contains("(Changed)"), "{text}");
        compact.validate_with_full(&full).unwrap();
    }

    #[test]
    fn ifds_fr004_conditional_swap_deduplicates_sources_and_preserves_control() {
        let before = "function result(flag) {\nlet one = 1;\nlet two = 2;\nif (flag) return one;\nreturn two;\n}";
        let after = before.replace("if (flag)", "if (!flag)");
        let full = assemble_function_result_report(analyze(
            before,
            &after,
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        assert_eq!(compact.findings.len(), 1);
        assert_eq!(text.matches("Return choice").count(), 1, "{text}");
        assert_eq!(text.matches("Effect:").count(), 2, "{text}");
        assert_eq!(text.matches("Control: flag -> !flag").count(), 1, "{text}");
        assert_eq!(
            text.matches("Edit attribution uncertain").count(),
            0,
            "{text}"
        );
        for side in ["Before", "After"] {
            assert_eq!(
                text.matches(&format!(
                    "{side} contributing writes: src/fixture.ts:2, src/fixture.ts:3"
                ))
                .count(),
                1,
                "{text}"
            );
        }
        assert_eq!(text, compact.render_text(&full));
        let restored_full =
            FunctionResultReport::from_json(&serde_json::to_vec(&full).unwrap()).unwrap();
        let restored_compact =
            FunctionResultCompactReport::from_json(&serde_json::to_vec(&compact).unwrap()).unwrap();
        assert_eq!(text, restored_compact.render_text(&restored_full));
    }

    #[test]
    fn ifds_fr004_summarizes_scoped_equal_choices_without_losing_changed_selection() {
        let (full, compact) = review("priority");
        let text = compact.render_text(&full);
        assert_eq!(
            text.matches("Control: guard order a then b -> b then a")
                .count(),
            1,
            "{text}"
        );
        assert!(
            text.contains("Unchanged data flow under !a || !b (Equal)"),
            "{text}"
        );
        assert_eq!(text.matches("Unchanged data flow").count(), 1, "{text}");
        assert!(
            text.contains("a && b: x (2) -> x (1) (Different)"),
            "{text}"
        );
        assert!(text.contains("x (1) | a && !b | a"), "{text}");
        assert!(text.contains("x (2) | b | !a && b"), "{text}");
        assert!(!text.contains("Normal results unchanged"), "{text}");
        assert!(
            text.len() < compact.render_verbose_text(&full).len(),
            "{text}"
        );
        compact.validate_with_full(&full).unwrap();
    }

    #[test]
    fn ifds_fr003_short_circuit_identity_report() {
        let (full, compact) = review("short_circuit");
        let text = compact.render_text(&full);
        assert!(
            text.contains("when flag is truthy: \"old\" -> \"new\" (Different)"),
            "{text}"
        );
        assert!(!text.contains("Equal"), "{text}");
        assert!(!text.contains("Unknown"), "{text}");
        assert!(!text.contains("Return choice"), "{text}");
        assert!(!text.contains("flag &&"), "{text}");
        assert_eq!(compact.equal_regions.len(), 1);
        assert_eq!(
            compact.equal_regions[0].region.values,
            BTreeMap::from([(0, false)])
        );
        assert_eq!(compact.coverage.result_comparison, Coverage::Complete);
        let equal = &full.comparison.regions[compact.equal_regions[0].evidence.index];
        assert_eq!(equal.proof, Some(ResultProof::PairedInputIdentity));
        assert_eq!(equal.before_result.as_deref(), Some("flag && \"old\""));
        assert_eq!(
            compact.equal_regions[0]
                .before_result
                .as_ref()
                .unwrap()
                .expression,
            "flag && \"old\""
        );
        compact.validate_with_full(&full).unwrap();

        // Unaffected results remain in JSON even when their source expressions
        // are unrelated to the changed branch's expressions.
        let full = assemble_function_result_report(enabled());
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        assert_eq!(
            compact.equal_regions.len(),
            full.comparison
                .regions
                .iter()
                .filter(|region| region.assessment == ResultAssessment::Equal)
                .count()
        );
        let text = compact.render_text(&full);
        assert!(!text.contains("Equal under"), "{text}");
        assert!(!text.contains("Unchanged choices"), "{text}");
    }

    #[test]
    fn ifds_fr003_proven_copy_and_priority() {
        let (full, compact) = review("copy");
        let text = compact.render_verbose_text(&full);
        assert!(
            text.contains("saved (1) -> saved (2) (Different)"),
            "{text}"
        );
        assert!(!text.contains("(9)"));
        assert!(text.contains("Before return saved at src/fixture.ts:5; contributing writes: src/fixture.ts:3, src/fixture.ts:2"), "{text}");
        assert!(text.contains("After return saved at src/fixture.ts:5; contributing writes: src/fixture.ts:3, src/fixture.ts:2"), "{text}");
        assert!(
            !text.contains("src/fixture.ts:4"),
            "the later overwrite is not a source: {text}"
        );
        let effect = &compact.findings[0].effects[0];
        for (result, expected) in [(&effect.before_result, "1"), (&effect.after_result, "2")] {
            let result = result.as_ref().unwrap();
            assert_eq!(
                result.proven_value,
                Some(FunctionKnownValue::Number {
                    value: expected.into()
                })
            );
            assert_eq!(
                result.evidence[0]
                    .dependencies
                    .iter()
                    .filter(|dependency| dependency.operation == "write")
                    .count(),
                2
            );
        }
        let (full, compact) = review("priority");
        let text = compact.render_verbose_text(&full);
        assert!(
            text.contains("a && b: x (2) -> x (1) (Different)"),
            "{text}"
        );
        assert!(text.contains("x (1) | a && !b | a"), "{text}");
        assert_eq!(
            text.matches("contributing writes: src/fixture.ts:4")
                .count(),
            2,
            "{text}"
        );
        assert!(text.contains("x (2) | b | !a && b"), "{text}");
        assert!(!text.contains("Equal under"), "{text}");
        assert!(!text.contains("Unchanged choices"), "{text}");
        assert_eq!(compact.equal_regions.len(), 3);
        assert_eq!(compact.findings.len(), 1);
    }

    #[test]
    fn ifds_fr003_readable_equal_control() {
        let (full, compact) = review("equal_control");
        let before_full = serde_json::to_vec(&full).unwrap();
        let before_compact = serde_json::to_vec(&compact).unwrap();
        let text = compact.render_text(&full);
        assert!(
            text.contains("Control: flag -> !flag; result remains 1 (Equal)"),
            "{text}"
        );
        assert_eq!(text.matches("Control:").count(), 1, "{text}");
        assert_eq!(
            text.matches("Edit attribution uncertain").count(),
            1,
            "{text}"
        );
        for absent in [
            "Return choice",
            "Effect:",
            "Equal under",
            "Unchanged choices",
            "Normal results unchanged",
        ] {
            assert!(!text.contains(absent), "{text}");
        }
        assert!(
            text.contains("Coverage: result comparison Complete; source alignment Partial"),
            "{text}"
        );
        assert_eq!(before_full, serde_json::to_vec(&full).unwrap());
        assert_eq!(before_compact, serde_json::to_vec(&compact).unwrap());
        compact.validate_with_full(&full).unwrap();
    }

    #[test]
    fn ifds_fr003_scoped_and_unresolved_equal_control() {
        for (before, after, concrete) in [
            (
                "function result(enabled, flag) { if (enabled) { if (flag) return 1; return 1; } return 2; }",
                "function result(enabled, flag) { if (enabled) { if (!flag) return 1; return 1; } return 2; }",
                true,
            ),
            (
                "function result(flag, ready) { if (flag) return 1; return 1; }",
                "function result(flag, ready) { if (ready) return 1; return 1; }",
                false,
            ),
            (
                "function result(flag) { if (flag) return 1; return 1; }",
                "function result(renamed) { if (!renamed) return 1; return 1; }",
                true,
            ),
            (
                "function result(flag, other) { if (flag) return 1; if (other) return 1; return 1; }",
                "function result(flag, other) { if (other) return 1; if (!flag) return 1; return 1; }",
                false,
            ),
        ] {
            let full = assemble_function_result_report(analyze(
                before,
                after,
                FunctionEntry::default(),
                FunctionEntry::default(),
            ));
            let compact =
                compact_function_result_report(&full, FunctionPresentationOptions::default());
            let text = compact.render_text(&full);
            assert!(text.contains("(Equal)"), "{text}");
            assert!(!text.contains("Normal results unchanged"), "{text}");
            if concrete {
                assert!(text.contains("Control:"), "{text}");
                assert!(!text.contains("correspondence unresolved"), "{text}");
                if before.contains("enabled") {
                    assert!(
                        text.contains("under enabled in the common input scope"),
                        "{text}"
                    );
                    let control = text
                        .lines()
                        .find(|line| line.starts_with("Control:"))
                        .unwrap();
                    assert!(!control.contains("under always"), "{text}");
                } else {
                    assert!(text.contains("flag -> !renamed"), "{text}");
                }
            } else {
                assert!(
                    text.contains("Control changed (guard correspondence unresolved)"),
                    "{text}"
                );
                assert!(!text.contains("flag -> ready"), "{text}");
            }
            compact.validate_with_full(&full).unwrap();
        }
    }

    #[test]
    fn ifds_fr003_complete_unchanged_summary() {
        let (full, compact) = review("overwritten");
        let text = compact.render_text(&full);
        assert_eq!(
            text.matches("Normal results unchanged").count(),
            1,
            "{text}"
        );
        assert!(text.contains("on common inputs: always"), "{text}");
        assert!(text.contains("source alignment Partial"), "{text}");
        assert!(!text.contains("Equal under"), "{text}");
        assert!(!text.contains("Unchanged choices"), "{text}");

        let mut entry = FunctionEntry::default();
        entry
            .known_values
            .insert(0, FunctionKnownValue::Boolean { value: true });
        let full = assemble_function_result_report(analyze(
            "function result(flag) { return flag; }",
            "function result(flag) { const copy = flag; return copy; }",
            entry.clone(),
            entry,
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        assert!(text.contains("Entry scope:"), "{text}");
        assert!(text.contains("flag = Boolean { value: true }"), "{text}");
        assert!(text.contains("Normal results unchanged"), "{text}");
        assert!(!text.contains("(Equal)"), "{text}");
    }

    #[test]
    fn ifds_fr003_equal_control_alongside_changed_results() {
        let before =
            "function result(mode, flag) { if (mode) { if (flag) return 1; return 1; } return 2; }";
        let after = "function result(mode, flag) { if (mode) { if (!flag) return 1; return 1; } return 3; }";
        let analysis = analyze(
            before,
            after,
            FunctionEntry::default(),
            FunctionEntry::default(),
        );
        let full = assemble_function_result_report(analysis.clone());
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        assert!(
            compact
                .findings
                .iter()
                .all(|finding| finding.assessment != ResultAssessment::Equal)
        );
        let text = compact.render_text(&full);
        assert!(
            text.contains("Control: resolved: flag -> resolved: !flag"),
            "{text}"
        );
        assert!(
            text.contains("result remains 1 (Equal) under mode"),
            "{text}"
        );
        assert!(text.contains("2 -> 3 (Different)"), "{text}");
        assert!(!text.contains("Normal results unchanged"), "{text}");
        let mut reordered = analysis;
        reordered.before.as_mut().unwrap().observations.reverse();
        reordered.after.as_mut().unwrap().observations.reverse();
        let reordered_full = assemble_function_result_report(reordered);
        let reordered_compact =
            compact_function_result_report(&reordered_full, FunctionPresentationOptions::default());
        assert_eq!(compact, reordered_compact);
        assert_eq!(text, reordered_compact.render_text(&reordered_full));
    }

    #[test]
    fn ifds_fr003_no_false_unchanged_summary() {
        let analysis = analyze(
            "function result(flag) { if (flag) return 1; return 2; }",
            "function result(flag) { if (flag) return 1; return 2; }",
            FunctionEntry::default(),
            FunctionEntry::default(),
        );
        let full = assemble_function_result_report_with_comparison_limit(analysis, 1);
        assert!(
            full.comparison
                .regions
                .iter()
                .all(|region| region.assessment == ResultAssessment::Equal)
        );
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        assert!(!text.contains("Normal results unchanged"), "{text}");
        assert!(text.contains("result comparison Partial"), "{text}");
        assert!(
            text.contains("comparison limits: comparison_assignments"),
            "{text}"
        );

        let (full, _) = review("equal_control");
        let compact =
            compact_function_result_report(&full, FunctionPresentationOptions { max_groups: 0 });
        let text = compact.render_text(&full);
        assert!(!text.contains("Normal results unchanged"), "{text}");
        assert!(!text.contains("Control:"), "{text}");
        assert!(
            text.contains("presentation Partial; 1 groups omitted"),
            "{text}"
        );
        assert!(text.contains("Evidence:"), "{text}");

        for name in ["unknown", "expression"] {
            let (full, compact) = review(name);
            let text = compact.render_text(&full);
            assert!(!text.contains("Normal results unchanged"), "{text}");
            if name == "unknown" {
                assert!(text.contains("Unknown under"), "{text}");
            } else {
                assert!(text.contains("(Changed)"), "{text}");
                assert!(!text.contains("Unknown under"), "{text}");
            }
        }
    }

    #[test]
    fn ifds_fr003_unknown_and_presence_summary_boundaries() {
        let full = assemble_function_result_report(analyze(
            "function result() { return 1; }",
            "",
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        assert!(text.contains("Function removed"), "{text}");
        assert!(!text.contains("Normal results unchanged"), "{text}");

        let mut before = FunctionEntry::default();
        before
            .known_values
            .insert(0, FunctionKnownValue::Boolean { value: true });
        let mut after = before.clone();
        after
            .known_values
            .insert(0, FunctionKnownValue::Boolean { value: false });
        let full = assemble_function_result_report(analyze(
            "function result(flag) { return 1; }",
            "function result(flag) { return 1; }",
            before,
            after,
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        assert!(text.contains("No common inputs"), "{text}");
        assert!(!text.contains("Normal results unchanged"), "{text}");
        assert!(!text.contains("(Equal)"), "{text}");

        let mut analysis = analyze(
            "function result(flag) { return flag; }",
            "function result(flag) { return flag; }",
            FunctionEntry::default(),
            FunctionEntry::default(),
        );
        analysis.input_correspondence.clear();
        analysis.input_mapping_coverage = Coverage::Partial;
        let full = assemble_function_result_report(analysis);
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        assert!(!text.contains("Normal results unchanged"), "{text}");
        assert!(text.contains("input mapping Partial"), "{text}");
        assert!(text.contains("Unknown under"), "{text}");
    }

    #[test]
    fn ifds_fr003_attribution_marker_per_finding() {
        for name in ["copy", "guard_version", "unknown", "known_expression"] {
            let (full, compact) = review(name);
            let text = compact.render_text(&full);
            let expected = compact
                .findings
                .iter()
                .filter(|finding| {
                    finding.assessment != ResultAssessment::Equal && !finding.attribution_certain
                })
                .count()
                + full
                    .findings
                    .iter()
                    .filter(|finding| {
                        finding.assessment == ResultAssessment::Unknown
                            && !finding.attribution_certain
                    })
                    .count();
            assert_eq!(
                text.matches("Edit attribution uncertain").count(),
                expected,
                "{name}: {text}"
            );
            if name == "guard_version" {
                assert_eq!(expected, 1);
                assert_eq!(text.matches("(Different)").count(), 2, "{text}");
            }
            compact.validate_with_full(&full).unwrap();
        }
        let (full, compact) = review("literal");
        assert!(compact.findings[0].attribution_certain);
        let text = compact.render_text(&full);
        assert!(!text.contains("Edit attribution uncertain"), "{text}");
        assert!(text.contains("1 -> 2 (Different)"), "{text}");
    }

    #[test]
    fn ifds_fr003_proven_values_and_boundaries() {
        for name in [
            "literal",
            "fallthrough",
            "overwritten",
            "expression",
            "known_expression",
            "unknown",
        ] {
            let (full, compact) = review(name);
            let text = compact.render_text(&full);
            match name {
                "literal" => {
                    assert!(text.contains("1 -> 2 (Different)"), "{text}");
                    assert!(!text.contains("1 (1)"));
                }
                "fallthrough" => {
                    assert!(text.contains("undefined -> 2 (Different)"), "{text}");
                    assert!(!text.contains("undefined (undefined)"));
                }
                "overwritten" => {
                    assert!(!text.contains("x (9)"), "{text}");
                    assert_eq!(
                        compact.equal_regions[0]
                            .before_result
                            .as_ref()
                            .unwrap()
                            .display(),
                        "x (9)"
                    );
                }
                "expression" => {
                    assert!(text.contains("input + 1 -> input + 2 (Changed)"), "{text}");
                    assert_eq!(
                        compact.findings[0].effects[0]
                            .before_result
                            .as_ref()
                            .unwrap()
                            .proven_value,
                        None
                    );
                }
                "known_expression" => {
                    assert!(text.contains("input + 1 -> input + 2 (Changed)"), "{text}");
                    assert!(!text.contains("(4)"));
                    assert!(!text.contains("(5)"));
                }
                "unknown" => {
                    assert!(text.contains("1 -> 2 (Different)"), "{text}");
                    assert!(text.contains("Unknown under !flag"), "{text}");
                    assert_eq!(compact.coverage.result_comparison, Coverage::Partial);
                    assert_eq!(
                        compact.unknown_regions[0]
                            .before_result
                            .as_ref()
                            .unwrap()
                            .proven_value,
                        None
                    );
                }
                _ => unreachable!(),
            }
        }
        let mut entry = FunctionEntry::default();
        entry
            .known_values
            .insert(0, FunctionKnownValue::Number { value: "4".into() });
        let full = assemble_function_result_report(analyze(
            "function result(input) { return input; }",
            "function result(input) { const saved = input; return saved; }",
            entry.clone(),
            entry,
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        assert!(!text.contains("(4)"), "{text}");
        let equal = &compact.equal_regions[0];
        assert_eq!(equal.before_result.as_ref().unwrap().display(), "input (4)");
        assert_eq!(equal.after_result.as_ref().unwrap().display(), "saved (4)");
        let full = assemble_function_result_report(analyze(
            "function result(flag: boolean) { if (flag) return flag; return flag; }",
            "function result(flag: boolean) { if (flag) return flag; return flag; }",
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        assert!(
            compact.before_flow.iter().all(|flow| flow
                .result
                .as_ref()
                .unwrap()
                .proven_value
                .is_none())
        );
    }

    #[test]
    fn ifds_fr003_symmetric_guard_evidence() {
        let (full, compact) = review("guard_version");
        assert_eq!(compact.findings.len(), 1, "{compact:#?}");
        assert_eq!(compact.findings[0].effects.len(), 2);
        let text = compact.render_text(&full);
        assert_eq!(
            text.matches("Return choice | Before | After").count(),
            1,
            "{text}"
        );
        assert!(
            text.contains("!flag: \"no\" -> \"yes\" (Different)"),
            "{text}"
        );
        assert!(
            text.contains("flag: \"yes\" -> \"no\" (Different)"),
            "{text}"
        );
        assert_ne!(
            compact.findings[0].effects[0].evidence,
            compact.findings[0].effects[1].evidence
        );

        // Identical tables in disjoint outer branches come from two distinct edits.
        let before = "function result(outer, flag) { if (outer) { if (flag) return 'yes'; return 'no'; } if (flag) return 'yes'; return 'no'; }";
        let after = "function result(outer, flag) { if (outer) { if (!flag) return 'yes'; return 'no'; } if (!flag) return 'yes'; return 'no'; }";
        let full = assemble_function_result_report(analyze(
            before,
            after,
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        assert!(compact.findings.len() >= 2, "{compact:#?}");
        for finding in &compact.findings {
            let regions: Vec<_> = finding
                .effects
                .iter()
                .map(|effect| &full.comparison.regions[effect.evidence.index].region.values)
                .collect();
            assert!(
                regions
                    .iter()
                    .all(|region| region.get(&0) == regions[0].get(&0)),
                "{finding:#?}"
            );
        }
        compact.validate_with_full(&full).unwrap();
    }

    #[test]
    fn ifds_fr003_typed_value_validation() {
        let (full, compact) = review("copy");
        assert_eq!(full.schema_version, 3);
        assert_eq!(full.analysis.schema_version, 4);
        assert_eq!(compact.schema_version, 3);
        assert_eq!(
            FunctionResultCompactReport::from_json(&serde_json::to_vec(&compact).unwrap()).unwrap(),
            compact
        );
        let mut corrupted = compact.clone();
        corrupted.findings[0].effects[0]
            .before_result
            .as_mut()
            .unwrap()
            .proven_value = Some(FunctionKnownValue::Number { value: "9".into() });
        assert!(corrupted.validate_with_full(&full).is_err());
        let mut corrupted = compact.clone();
        corrupted.findings[0].effects[0]
            .before_result
            .as_mut()
            .unwrap()
            .evidence[0]
            .dependencies[0]
            .path
            .push(usize::MAX);
        assert!(corrupted.validate_with_full(&full).is_err());
        let mut old = serde_json::to_value(&compact).unwrap();
        let mut corrupted = compact.clone();
        corrupted.findings[0].effects[0]
            .before_result
            .as_mut()
            .unwrap()
            .evidence[0]
            .dependencies[0]
            .span
            .as_mut()
            .unwrap()
            .start_line = 4;
        assert!(corrupted.validate_with_full(&full).is_err());
        old["schema_version"] = 1.into();
        assert!(
            FunctionResultCompactReport::from_json(&serde_json::to_vec(&old).unwrap()).is_err()
        );
        let mut old_full = serde_json::to_value(&full).unwrap();
        old_full["schema_version"] = 1.into();
        assert!(FunctionResultReport::from_json(&serde_json::to_vec(&old_full).unwrap()).is_err());
        old_full["schema_version"] = 2.into();
        old_full["analysis"]["schema_version"] = 3.into();
        assert!(FunctionResultReport::from_json(&serde_json::to_vec(&old_full).unwrap()).is_err());

        let mut domain = FunctionEntry::default();
        domain.domains.insert(0, PrimitiveDomain::Number);
        let full = assemble_function_result_report(analyze(
            "function result(p) { return p; }",
            "function result(p) { return 1; }",
            domain.clone(),
            domain,
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let effect = &compact.findings[0].effects[0];
        assert_eq!(effect.before_result.as_ref().unwrap().proven_value, None);
        assert_eq!(
            effect.after_result.as_ref().unwrap().proven_value,
            Some(FunctionKnownValue::Number { value: "1".into() })
        );
        assert!(compact.render_text(&full).contains("p -> 1 (Changed)"));
    }

    #[test]
    fn ifds_fr003_primitive_annotations_and_symbolic_copies() {
        for (literal, expected) in [
            ("true", FunctionKnownValue::Boolean { value: true }),
            (
                "'a\"b'",
                FunctionKnownValue::String {
                    value: "a\"b".into(),
                },
            ),
            ("null", FunctionKnownValue::Null),
        ] {
            let source = format!("function result() {{ const saved = {literal}; return saved; }}");
            let full = assemble_function_result_report(analyze(
                &source,
                &source,
                FunctionEntry::default(),
                FunctionEntry::default(),
            ));
            let compact =
                compact_function_result_report(&full, FunctionPresentationOptions::default());
            assert!(
                !compact.equal_regions.is_empty(),
                "{literal}: {:#?}",
                full.comparison
            );
            assert_eq!(
                compact.equal_regions[0]
                    .before_result
                    .as_ref()
                    .unwrap()
                    .proven_value,
                Some(expected)
            );
            compact.validate_with_full(&full).unwrap();
            if literal.starts_with('\'') {
                assert_eq!(
                    compact.equal_regions[0]
                        .before_result
                        .as_ref()
                        .unwrap()
                        .display(),
                    "saved (\"a\\\"b\")"
                );
                assert!(!compact.render_text(&full).contains("saved"));
            }
        }
        let mut entry = FunctionEntry::default();
        entry.known_values.insert(0, FunctionKnownValue::Undefined);
        let full = assemble_function_result_report(analyze(
            "function result(input) { const saved = input; return saved; }",
            "function result(input) { const saved = input; return saved; }",
            entry.clone(),
            entry,
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        assert_eq!(
            compact.equal_regions[0]
                .before_result
                .as_ref()
                .unwrap()
                .display(),
            "saved (undefined)"
        );
        assert!(!compact.render_text(&full).contains("saved"));

        let mut domain = FunctionEntry::default();
        domain
            .domains
            .extend([(0, PrimitiveDomain::String), (1, PrimitiveDomain::String)]);
        let full = assemble_function_result_report(analyze(
            "function result(p, q) { const saved = p; return saved; }",
            "function result(p, q) { const saved = q; return saved; }",
            domain.clone(),
            domain,
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        assert!(compact.unchanged_choices.is_empty());
        assert_eq!(compact.findings[0].choices.len(), 2);
        let effect = &compact.findings[0].effects[0];
        assert_ne!(
            effect.before_result.as_ref().unwrap().identity,
            effect.after_result.as_ref().unwrap().identity
        );
        assert!(
            compact
                .render_text(&full)
                .contains("saved -> saved (Changed)")
        );

        let full = assemble_function_result_report(analyze(
            "function result(flag) { if (flag) return 'yes'; return 'no'; }",
            "function result(flag) { if (flag) return 'no'; return 'yes'; }",
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        assert_eq!(
            compact.findings.len(),
            2,
            "swapped labels do not establish a guard change"
        );
        let (full, compact) = review("guard_version");
        let omitted =
            compact_function_result_report(&full, FunctionPresentationOptions { max_groups: 0 });
        assert_eq!(omitted.coverage.omitted_groups, 1);
        assert_eq!(omitted.coverage.presentation, Coverage::Partial);
        omitted.validate_with_full(&full).unwrap();
        let mut permuted = full.analysis.clone();
        permuted.before.as_mut().unwrap().observations.reverse();
        permuted.after.as_mut().unwrap().observations.reverse();
        let reordered = assemble_function_result_report(permuted);
        assert_eq!(
            compact,
            compact_function_result_report(&reordered, FunctionPresentationOptions::default())
        );
    }

    #[test]
    fn ifds_fr003_api_round_trip() {
        let full = assemble_function_result_report(enabled());
        let bytes = serde_json::to_vec(&full).unwrap();
        assert_eq!(FunctionResultReport::from_json(&bytes).unwrap(), full);
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        assert_eq!(
            FunctionResultCompactReport::from_json(&serde_json::to_vec(&compact).unwrap()).unwrap(),
            compact
        );
        assert_eq!(full.comparison.regions.len(), 3);
        assert!(
            full.analysis
                .before
                .as_ref()
                .unwrap()
                .observations
                .iter()
                .all(|observation| observation.caller_continuation_open)
        );
        let mut changed_scope = FunctionEntry::default();
        changed_scope.domains.insert(0, PrimitiveDomain::Boolean);
        changed_scope
            .known_values
            .insert(0, FunctionKnownValue::Boolean { value: true });
        let scoped = assemble_function_result_report(analyze(
            "function answer(flag: boolean) { return 1; }",
            "function answer(flag: boolean) { return 1; }",
            changed_scope,
            FunctionEntry::default(),
        ));
        assert_eq!(
            FunctionResultReport::from_json(&serde_json::to_vec(&scoped).unwrap()).unwrap(),
            scoped
        );
        assert_ne!(
            scoped.comparison.before_entry,
            scoped.comparison.after_entry
        );
    }

    #[test]
    fn ifds_fr003_text_json_parity() {
        let full = assemble_function_result_report(enabled());
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        assert!(
            text.contains("enabled && !ready: \"ok\" -> \"skip\""),
            "{text}"
        );
        assert_eq!(compact.findings.len(), 1);
        assert_eq!(compact.findings[0].assessment, ResultAssessment::Different);
        assert_eq!(compact.equal_regions.len(), 2);
        assert_eq!(compact.before_entry, FunctionEntry::default());
        assert_eq!(compact.after_entry, FunctionEntry::default());
        compact.validate_with_full(&full).unwrap();
        let mut before_entry = FunctionEntry::default();
        before_entry.domains.insert(0, PrimitiveDomain::Boolean);
        before_entry
            .known_values
            .insert(0, FunctionKnownValue::Boolean { value: true });
        let mut after_entry = FunctionEntry::default();
        after_entry.domains.insert(0, PrimitiveDomain::Boolean);
        after_entry
            .known_values
            .insert(0, FunctionKnownValue::Boolean { value: false });
        let full = assemble_function_result_report(analyze(
            "function answer(flag: boolean) { if (flag) return \"old\"; return \"off\"; }",
            "function answer(flag: boolean) { if (flag) return \"new\"; return \"off\"; }",
            before_entry,
            after_entry,
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        assert_eq!(compact.common_domain_status, CommonDomainStatus::Empty);
        assert!(compact.findings.is_empty());
        assert!(compact.equal_regions.is_empty());
        let text = compact.render_text(&full);
        assert!(text.contains("No common inputs"));
        assert!(text.contains("\"old\""));
        assert!(text.contains("\"off\""));
        assert!(!text.contains("\"old\" -> \"off\""));
        assert_eq!(compact.before_flow.len(), 1);
        assert_eq!(compact.after_flow.len(), 1);
        compact.validate_with_full(&full).unwrap();

        let mut before_scope = FunctionEntry::default();
        before_scope
            .domains
            .extend([(0, PrimitiveDomain::Boolean), (1, PrimitiveDomain::Boolean)]);
        before_scope.assumptions.push(AssumptionExpr::Binary {
            operator: AssumptionBinary::And,
            left: Box::new(AssumptionExpr::Input { index: 0 }),
            right: Box::new(AssumptionExpr::Input { index: 1 }),
        });
        let mut after_scope = before_scope.clone();
        after_scope.assumptions = vec![AssumptionExpr::Input { index: 0 }];
        let full = assemble_function_result_report(analyze(
            "function answer(flag: boolean, ready: boolean) { return 1; }",
            "function answer(flag: boolean, ready: boolean) { return 1; }",
            before_scope,
            after_scope,
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        let text = compact.render_text(&full);
        assert!(
            text.contains("Before scope: flag: Boolean; ready: Boolean; (flag && ready)"),
            "{text}"
        );
        assert!(text.contains("After only: flag && !ready"), "{text}");
        assert!(compact.findings.is_empty());

        let mut number = FunctionEntry::default();
        number.domains.insert(0, PrimitiveDomain::Number);
        let full = assemble_function_result_report(analyze(
            "function next(p: number) { return p + 1; }",
            "function next(p: number) { return p + 2; }",
            number.clone(),
            number,
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        assert!(
            compact
                .findings
                .iter()
                .any(|finding| finding.assessment == ResultAssessment::Changed)
        );
        let text = compact.render_text(&full);
        assert!(text.contains("p + 1 -> p + 2 (Changed)"), "{text}");

        let full = assemble_function_result_report(analyze(
            "function oldValue() { return 1; }",
            "",
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        assert_eq!(compact.function_presence, FunctionPresence::BeforeOnly);
        assert!(compact.render_text(&full).contains("Function removed"));
        assert!(compact.findings.is_empty());
        assert!(compact.equal_regions.is_empty());
    }

    #[test]
    fn ifds_fr003_deterministic_evidence() {
        let original = enabled();
        let mut permuted = original.clone();
        permuted.before.as_mut().unwrap().observations.reverse();
        permuted.after.as_mut().unwrap().observations.reverse();
        let one = assemble_function_result_report(original);
        let two = assemble_function_result_report(permuted);
        assert_eq!(
            serde_json::to_vec(&one).unwrap(),
            serde_json::to_vec(&two).unwrap()
        );
        let compact = compact_function_result_report(&one, FunctionPresentationOptions::default());
        compact.validate_with_full(&one).unwrap();
        let mut corrupted = compact.clone();
        corrupted.findings[0].evidence[0].index = usize::MAX;
        assert!(corrupted.validate_with_full(&one).is_err());
    }

    #[test]
    fn ifds_fr003_compact_grouping() {
        let full = assemble_function_result_report(analyze(
            "function status(enabled: boolean, blocked: boolean, ready: boolean, approved: boolean) { if (!enabled) return \"disabled\"; if (blocked) return \"blocked\"; if (ready) return \"ok\"; return \"pending\"; }",
            "function status(enabled: boolean, blocked: boolean, ready: boolean, approved: boolean) { if (!enabled) return \"disabled\"; if (blocked) return \"blocked\"; if (ready && approved) return \"ok\"; return \"pending\"; }",
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        assert_eq!(compact.findings.len(), 1, "{compact:#?}");
        assert_eq!(
            compact.findings[0].context.values,
            BTreeMap::from([(0, true), (1, false)])
        );
        assert_eq!(
            compact.findings[0].effect[0].values,
            BTreeMap::from([(2, true), (3, false)])
        );
        assert_eq!(
            compact
                .unchanged_choices
                .iter()
                .map(PresentedResult::display)
                .collect::<Vec<_>>(),
            ["\"blocked\"", "\"disabled\""]
        );
        let text = compact.render_text(&full);
        assert!(
            text.contains("\"pending\" | !ready | !ready || !approved"),
            "{text}"
        );
        assert_eq!(text.matches("Context: enabled && !blocked").count(), 1);
    }

    #[test]
    fn ifds_fr003_attribution_and_scope() {
        let full = assemble_function_result_report(analyze(
            "function value(flag: boolean) { if (flag) return 1; return 2; }",
            "function value(flag: boolean) { if (flag) return 1; return 3; }",
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let finding = full
            .findings
            .iter()
            .find(|finding| finding.assessment == ResultAssessment::Different)
            .unwrap();
        assert!(!finding.before_locations.is_empty());
        assert!(!finding.after_locations.is_empty());
        assert!(finding.attribution_certain);
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        assert!(compact.coverage.caller_continuation_open);
    }

    #[test]
    fn ifds_fr003_coverage_and_truncation() {
        let analysis = enabled();
        let full = assemble_function_result_report_with_comparison_limit(analysis, 2);
        assert_eq!(full.comparison.comparison_coverage, Coverage::Partial);
        assert!(!full.comparison.regions.is_empty());
        assert!(
            full.comparison
                .limits_hit
                .contains("comparison_assignments")
        );
        let compact =
            compact_function_result_report(&full, FunctionPresentationOptions { max_groups: 0 });
        assert_eq!(compact.coverage.presentation, Coverage::Partial);
        assert!(compact.coverage.omitted_groups > 0);
        assert!(compact.unchanged_choices.is_empty());
        assert!(
            compact
                .render_text(&full)
                .contains("Coverage: result comparison Partial")
        );
        let full = assemble_function_result_report(analyze(
            "function value(flag: boolean) { if (flag) return 1; mystery(); return 1; }",
            "function value(flag: boolean) { if (flag) return 2; mystery(); return 1; }",
            FunctionEntry::default(),
            FunctionEntry::default(),
        ));
        let compact = compact_function_result_report(&full, FunctionPresentationOptions::default());
        assert!(
            compact
                .unknown_regions
                .iter()
                .any(|region| region.reason.is_some())
        );
        assert!(compact.unchanged_choices.is_empty());
        assert!(compact.render_text(&full).contains("Unknown under"));
    }

    #[test]
    fn ifds_fr003_binding_compatibility() {
        let before = "function value() { let x = 1; return x; }";
        let after = "function value() { let x = 2; return x; }";
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
            repository_id: "binding-fixture".into(),
        };
        let after_handle = SnapshotHandle {
            id: after_id.clone(),
            repository_id: "binding-fixture".into(),
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
        let selector = |snapshot: SnapshotId, source: &str| {
            let index = index_bindings(snapshot.clone(), path, source).unwrap();
            let binding = index
                .bindings
                .iter()
                .find(|binding| binding.name == "x")
                .unwrap();
            crate::ifds::model::BindingSelector {
                snapshot,
                declaration: binding.declaration.clone(),
                expected_name: Some("x".into()),
                expected_enclosing_symbol: binding.enclosing_symbol.clone(),
            }
        };
        let capabilities = CapabilitySet {
            stage: 1,
            capabilities: BTreeSet::new(),
            version: "test".into(),
        };
        let query = crate::ifds::model::VariableFlowQuery {
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
            selected_binding: selector(before_id, before),
            counterpart: Some(selector(after_id, after)),
            entry: crate::ifds::model::EntryPoint::ContainingFunction,
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
        let (full, compact) = crate::ifds::analysis::analyze_variable_flow_reports(
            query,
            &provider,
            &environment,
            &environment,
        )
        .unwrap();
        assert_eq!(
            full.schema_version,
            crate::ifds::model::REPORT_SCHEMA_VERSION
        );
        assert_eq!(
            compact.schema_version,
            crate::ifds::compact::COMPACT_SCHEMA_VERSION
        );
        assert_eq!(compact.selected_binding.expected_name.as_deref(), Some("x"));
        assert!(compact.render_text(&full).contains("x = 1"));
        compact.validate_with_full(&full).unwrap();
    }
}
