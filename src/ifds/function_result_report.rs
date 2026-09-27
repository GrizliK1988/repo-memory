//! Versioned full evidence and compact presentation for function results.

use super::compact::EvidenceRef;
use super::function_result_comparison::*;
use super::function_results::*;
use super::model::{Coverage, SnapshotId, SnapshotSide, SourceSpan};
use super::snapshots::{AnalysisEnvironment, SnapshotProvider};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const FUNCTION_RESULT_REPORT_SCHEMA_VERSION: u32 = 1;
pub const FUNCTION_RESULT_COMPACT_SCHEMA_VERSION: u32 = 1;

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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionResultReport {
    pub schema_version: u32,
    pub analysis: FunctionResultAnalysis,
    pub comparison: FunctionResultComparison,
    pub findings: Vec<FunctionResultFinding>,
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
        Ok(report)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionChoiceConditions {
    pub result: String,
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
    pub before_result: Option<String>,
    pub after_result: Option<String>,
    pub assessment: ResultAssessment,
    pub control_changed: bool,
    pub value_dependency_changed: bool,
    pub return_structure_changed: bool,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactResultRegion {
    pub region: BooleanRegion,
    pub before_result: Option<String>,
    pub after_result: Option<String>,
    pub assessment: ResultAssessment,
    pub reason: Option<String>,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactSideFlow {
    pub flow: SnapshotResultFlow,
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
    pub unchanged_choices: Vec<String>,
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
    let findings = findings(&analysis, &comparison);
    FunctionResultReport {
        schema_version: FUNCTION_RESULT_REPORT_SCHEMA_VERSION,
        analysis,
        comparison,
        findings,
    }
}

pub fn assemble_function_result_report_with_comparison_limit(
    analysis: FunctionResultAnalysis,
    max_assignments: usize,
) -> FunctionResultReport {
    let analysis = canonicalize_analysis(analysis);
    let comparison = compare_function_results_with_limit(&analysis, max_assignments);
    let findings = findings(&analysis, &comparison);
    FunctionResultReport {
        schema_version: FUNCTION_RESULT_REPORT_SCHEMA_VERSION,
        analysis,
        comparison,
        findings,
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

fn choice_regions(flows: &[SnapshotResultFlow], result: &str) -> Vec<BooleanRegion> {
    let mut regions: Vec<_> = flows
        .iter()
        .filter(|flow| {
            flow.result.as_deref() == Some(result) && flow.completion == Coverage::Complete
        })
        .map(|flow| flow.region.clone())
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

pub fn compact_function_result_report(
    full: &FunctionResultReport,
    options: FunctionPresentationOptions,
) -> FunctionResultCompactReport {
    let id = report_id(full);
    let comparison = &full.comparison;
    let mut all_findings = Vec::new();
    let mut affected_choices = BTreeSet::new();
    let has_changed_result = full.findings.iter().any(|finding| {
        !matches!(
            finding.assessment,
            ResultAssessment::Equal | ResultAssessment::Unknown
        )
    });
    for finding in &full.findings {
        if finding.assessment == ResultAssessment::Unknown
            || (has_changed_result && finding.assessment == ResultAssessment::Equal)
        {
            continue;
        }
        let choices: BTreeSet<String> =
            [finding.before_result.clone(), finding.after_result.clone()]
                .into_iter()
                .flatten()
                .collect();
        if finding.assessment != ResultAssessment::Equal {
            affected_choices.extend(choices.iter().cloned());
        }
        let mut complete = Vec::new();
        for result in choices {
            complete.extend(choice_regions(&comparison.before_flow, &result));
            complete.extend(choice_regions(&comparison.after_flow, &result));
        }
        let context = common_context(&complete);
        let mut choices: Vec<_> = [finding.before_result.clone(), finding.after_result.clone()]
            .into_iter()
            .flatten()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|result| FunctionChoiceConditions {
                before: relative(choice_regions(&comparison.before_flow, &result), &context),
                after: relative(choice_regions(&comparison.after_flow, &result), &context),
                result,
            })
            .collect();
        choices.sort_by(|a, b| a.result.cmp(&b.result));
        let effect = relative(
            finding
                .regions
                .iter()
                .map(|index| comparison.regions[*index].region.clone())
                .collect(),
            &context,
        );
        all_findings.push(CompactResultFinding {
            context,
            choices,
            effect,
            before_result: finding.before_result.clone(),
            after_result: finding.after_result.clone(),
            assessment: finding.assessment,
            control_changed: finding.control_changed,
            value_dependency_changed: finding.value_dependency_changed,
            return_structure_changed: finding.return_structure_changed,
            evidence: finding
                .regions
                .iter()
                .map(|index| reference(&id, "comparison.regions", *index))
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
        .filter(|(_, region)| {
            region.assessment == ResultAssessment::Equal
                && (affected_choices.is_empty()
                    || region
                        .before_result
                        .as_ref()
                        .is_some_and(|r| affected_choices.contains(r)))
        })
        .map(|(index, region)| CompactResultRegion {
            region: region.region.clone(),
            before_result: region.before_result.clone(),
            after_result: region.after_result.clone(),
            assessment: region.assessment,
            reason: None,
            evidence: reference(&id, "comparison.regions", index),
        })
        .collect();
    let unknown_regions = comparison
        .regions
        .iter()
        .enumerate()
        .filter(|(_, region)| region.assessment == ResultAssessment::Unknown)
        .map(|(index, region)| CompactResultRegion {
            region: region.region.clone(),
            before_result: region.before_result.clone(),
            after_result: region.after_result.clone(),
            assessment: region.assessment,
            reason: region.unknown_reason.clone(),
            evidence: reference(&id, "comparison.regions", index),
        })
        .collect();
    let all_choices: BTreeSet<String> = comparison
        .before_flow
        .iter()
        .chain(&comparison.after_flow)
        .filter_map(|flow| flow.result.clone())
        .collect();
    let unchanged_choices = if comparison.comparison_coverage == Coverage::Complete {
        all_choices
            .into_iter()
            .filter(|result| {
                !affected_choices.contains(result)
                    && choice_regions(&comparison.before_flow, result)
                        == choice_regions(&comparison.after_flow, result)
            })
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
        before_flow: compact_flows(
            &id,
            "analysis.before.observations",
            &full.analysis.before,
            &comparison.before_flow,
        ),
        after_flow: compact_flows(
            &id,
            "analysis.after.observations",
            &full.analysis.after,
            &comparison.after_flow,
        ),
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
        debug_assert!(self.validate_with_full(full).is_ok());
        let names = &self.input_names;
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
                    flow.flow.result.as_deref().unwrap_or("unknown"),
                    flow.flow.completion
                ));
            }
            for flow in &self.after_flow {
                lines.push(format!(
                    "After {}: {} ({:?})",
                    region_text(&flow.flow.region, names),
                    flow.flow.result.as_deref().unwrap_or("unknown"),
                    flow.flow.completion
                ));
            }
        } else if self.common_domain_status == CommonDomainStatus::Empty {
            lines.push("Before flow:".into());
            for flow in &self.before_flow {
                lines.push(format!(
                    "  {}: {} ({:?})",
                    region_text(&flow.flow.region, names),
                    flow.flow.result.as_deref().unwrap_or("unknown"),
                    flow.flow.completion
                ));
            }
            lines.push("After flow:".into());
            for flow in &self.after_flow {
                lines.push(format!(
                    "  {}: {} ({:?})",
                    region_text(&flow.flow.region, names),
                    flow.flow.result.as_deref().unwrap_or("unknown"),
                    flow.flow.completion
                ));
            }
            lines.push("No common inputs; no cross-version result relation.".into());
        } else {
            if self.common_domain_status == CommonDomainStatus::Unresolved {
                lines.push("Common input feasibility is unresolved.".into());
            }
            for finding in &self.findings {
                let context = region_text(&finding.context, names);
                if !finding.context.values.is_empty() {
                    lines.push(format!("Context: {context}"));
                }
                lines.push("Return choice | Before | After".into());
                for choice in &finding.choices {
                    lines.push(format!(
                        "{} | {} | {}",
                        choice.result,
                        regions_text(&choice.before, names),
                        regions_text(&choice.after, names)
                    ));
                }
                lines.push(format!(
                    "Effect: {}: {} -> {} ({:?})",
                    regions_text(&finding.effect, names),
                    finding.before_result.as_deref().unwrap_or("unknown"),
                    finding.after_result.as_deref().unwrap_or("unknown"),
                    finding.assessment
                ));
            }
            if !self.unchanged_choices.is_empty() {
                lines.push(format!(
                    "Unchanged choices: {}",
                    self.unchanged_choices.join(", ")
                ));
            }
            for equal in &self.equal_regions {
                lines.push(format!(
                    "Equal under {}: {}",
                    region_text(&equal.region, names),
                    equal.before_result.as_deref().unwrap_or("unknown")
                ));
            }
            for unknown in &self.unknown_regions {
                lines.push(format!(
                    "Unknown under {}: {}",
                    region_text(&unknown.region, names),
                    unknown.reason.as_deref().unwrap_or("comparison unresolved")
                ));
            }
        }
        if self.coverage.result_comparison != Coverage::Complete
            || self.coverage.presentation != Coverage::Complete
        {
            lines.push(format!(
                "Coverage: comparison {:?}, presentation {:?}; {} groups omitted.",
                self.coverage.result_comparison,
                self.coverage.presentation,
                self.coverage.omitted_groups
            ));
        }
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
        assert_eq!(compact.unchanged_choices, ["\"blocked\"", "\"disabled\""]);
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
                .contains("Coverage: comparison Partial")
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
        assert!(compact.render_text().contains("x = 1"));
        compact.validate_with_full(&full).unwrap();
    }
}
