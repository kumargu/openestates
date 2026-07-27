use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::dag_config::{dag_root, load_fact_registry_index, load_json, DagConfigError};
use crate::knowledge::FactValue;
use crate::models::Property;
use crate::routes::enrichment::society_node_id;
use crate::serving::{LoadedServingBundle, ServingFactIndex, ServingFactRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactAvailability {
    Observed,
    Missing,
    Derived,
    NotApplicable,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingDataBehavior {
    Skip,
    PenalizeLightly,
    RequiresObserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoringMethod {
    LowerIsBetterRelativeToArea,
    EvidenceCoverage,
    TextOrDistancePresence,
    RiskLowerIsBetter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingDataPolicy {
    #[serde(default = "default_missing_behavior")]
    pub default_behavior: MissingDataBehavior,
    #[serde(default = "default_true")]
    pub log_gap: bool,
    #[serde(default = "default_true")]
    pub never_zero_fill: bool,
}

impl Default for MissingDataPolicy {
    fn default() -> Self {
        Self {
            default_behavior: default_missing_behavior(),
            log_gap: true,
            never_zero_fill: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringSignalPolicy {
    pub id: String,
    #[serde(default)]
    pub fact_keys: Vec<String>,
    #[serde(default)]
    pub fact_groups: Vec<String>,
    pub method: ScoringMethod,
    #[serde(default = "default_signal_weight")]
    pub weight: f64,
    #[serde(default = "default_missing_behavior")]
    pub missing: MissingDataBehavior,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScoringSurfacePolicy {
    #[serde(default)]
    pub enabled_signals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScoringSurfaces {
    #[serde(default)]
    pub search: ScoringSurfacePolicy,
    #[serde(default)]
    pub detail: ScoringSurfacePolicy,
    #[serde(default)]
    pub recommendations: ScoringSurfacePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationBranchPolicy {
    pub id: String,
    pub primary_signal: String,
    pub min_delta: f64,
    pub headline: String,
    pub lens: String,
    #[serde(default)]
    pub priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationRecallPolicy {
    #[serde(default = "default_recommendation_candidate_limit")]
    pub candidate_limit: usize,
    #[serde(default = "default_recommendation_branch_limit")]
    pub branch_limit: usize,
    #[serde(default = "default_recommendation_target_branch_count")]
    pub target_branch_count: usize,
    #[serde(default)]
    pub channels: Vec<RecommendationRecallChannelPolicy>,
    #[serde(default)]
    pub fallback_branch: RecommendationFallbackBranchPolicy,
    #[serde(default = "default_recommendation_tie_breakers")]
    pub tie_breakers: Vec<String>,
}

impl Default for RecommendationRecallPolicy {
    fn default() -> Self {
        Self {
            candidate_limit: default_recommendation_candidate_limit(),
            branch_limit: default_recommendation_branch_limit(),
            target_branch_count: default_recommendation_target_branch_count(),
            channels: Vec::new(),
            fallback_branch: RecommendationFallbackBranchPolicy::default(),
            tie_breakers: default_recommendation_tie_breakers(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationRecallChannelPolicy {
    pub id: String,
    pub kind: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_recommendation_channel_score")]
    pub score: f64,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub edge_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationFallbackBranchPolicy {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_recommendation_fallback_id")]
    pub id: String,
    #[serde(default = "default_recommendation_fallback_headline")]
    pub headline: String,
    #[serde(default = "default_recommendation_fallback_lens")]
    pub lens: String,
    #[serde(default = "default_recommendation_fallback_max_items")]
    pub max_items: usize,
}

impl Default for RecommendationFallbackBranchPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            id: default_recommendation_fallback_id(),
            headline: default_recommendation_fallback_headline(),
            lens: default_recommendation_fallback_lens(),
            max_items: default_recommendation_fallback_max_items(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringPolicyFile {
    pub version: u32,
    pub engine_version: String,
    #[serde(default)]
    pub missing_data: MissingDataPolicy,
    #[serde(default)]
    pub search_ranking: SearchRankingPolicy,
    #[serde(default)]
    pub fact_groups: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub runtime_fact_keys: Vec<String>,
    #[serde(default)]
    pub signals: Vec<ScoringSignalPolicy>,
    #[serde(default)]
    pub surfaces: ScoringSurfaces,
    #[serde(default)]
    pub recommendation_recall: RecommendationRecallPolicy,
    #[serde(default)]
    pub recommendation_branches: Vec<RecommendationBranchPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRankingPolicy {
    #[serde(default = "default_min_support_evidence_confidence")]
    pub min_support_evidence_confidence: f32,
    #[serde(default = "default_min_llm_evidence_confidence")]
    pub min_llm_evidence_confidence: f32,
    #[serde(default = "default_negative_no_data_penalty_multiplier")]
    pub negative_no_data_penalty_multiplier: f64,
    #[serde(default = "default_min_semantic_recall_score")]
    pub min_semantic_recall_score: f64,
    #[serde(default = "default_semantic_candidate_fit_weight")]
    pub semantic_candidate_fit_weight: f64,
    #[serde(default = "default_semantic_candidate_fit_cap")]
    pub semantic_candidate_fit_cap: f64,
    #[serde(default = "default_broad_local_recall_multiplier")]
    pub broad_local_recall_multiplier: usize,
    #[serde(default = "default_broad_local_recall_min_extra")]
    pub broad_local_recall_min_extra: usize,
    #[serde(default = "default_positive_evidence_floor_ratio")]
    pub positive_evidence_floor_ratio: f64,
    #[serde(default = "default_no_positive_evidence_score_multiplier")]
    pub no_positive_evidence_score_multiplier: f64,
    #[serde(default = "default_nearby_area_score_penalty")]
    pub nearby_area_score_penalty: f64,
    #[serde(default = "default_graph_area_score_penalty")]
    pub graph_area_score_penalty: f64,
    #[serde(default)]
    pub geo_distance_fact_keys: Vec<String>,
    #[serde(default = "default_nearby_distance_full_score_km")]
    pub nearby_distance_full_score_km: f64,
    #[serde(default = "default_nearby_distance_zero_score_km")]
    pub nearby_distance_zero_score_km: f64,
    #[serde(default = "default_nearby_distance_bonus_cap")]
    pub nearby_distance_bonus_cap: f64,
    #[serde(default = "default_named_place_full_score_km")]
    pub named_place_full_score_km: f64,
    #[serde(default = "default_named_place_zero_score_km")]
    pub named_place_zero_score_km: f64,
    #[serde(default = "default_named_place_score_weight")]
    pub named_place_score_weight: f64,
    #[serde(default = "default_named_place_distinctive_token_max_place_count")]
    pub named_place_distinctive_token_max_place_count: usize,
    #[serde(default = "default_named_place_distinctive_token_max_place_ratio")]
    pub named_place_distinctive_token_max_place_ratio: f64,
    #[serde(default = "default_named_place_generic_tokens")]
    pub named_place_generic_tokens: Vec<String>,
    #[serde(default = "default_named_place_query_stopwords")]
    pub named_place_query_stopwords: Vec<String>,
    #[serde(default = "default_review_rating_weight")]
    pub review_rating_weight: f64,
    #[serde(default = "default_review_count_weight")]
    pub review_count_weight: f64,
    #[serde(default = "default_review_count_log_divisor")]
    pub review_count_log_divisor: f64,
    #[serde(default = "default_min_score_with_positive_evidence")]
    pub min_score_with_positive_evidence: f64,
    #[serde(default = "default_max_score_with_positive_evidence")]
    pub max_score_with_positive_evidence: f64,
    #[serde(default = "default_min_score_with_risk_only_evidence")]
    pub min_score_with_risk_only_evidence: f64,
    #[serde(default = "default_min_score_with_constraint_only")]
    pub min_score_with_constraint_only: f64,
    #[serde(default = "default_fact_coverage_threshold")]
    pub fact_coverage_threshold: f64,
}

impl Default for SearchRankingPolicy {
    fn default() -> Self {
        Self {
            min_support_evidence_confidence: default_min_support_evidence_confidence(),
            min_llm_evidence_confidence: default_min_llm_evidence_confidence(),
            negative_no_data_penalty_multiplier: default_negative_no_data_penalty_multiplier(),
            min_semantic_recall_score: default_min_semantic_recall_score(),
            semantic_candidate_fit_weight: default_semantic_candidate_fit_weight(),
            semantic_candidate_fit_cap: default_semantic_candidate_fit_cap(),
            broad_local_recall_multiplier: default_broad_local_recall_multiplier(),
            broad_local_recall_min_extra: default_broad_local_recall_min_extra(),
            positive_evidence_floor_ratio: default_positive_evidence_floor_ratio(),
            no_positive_evidence_score_multiplier: default_no_positive_evidence_score_multiplier(),
            nearby_area_score_penalty: default_nearby_area_score_penalty(),
            graph_area_score_penalty: default_graph_area_score_penalty(),
            geo_distance_fact_keys: Vec::new(),
            nearby_distance_full_score_km: default_nearby_distance_full_score_km(),
            nearby_distance_zero_score_km: default_nearby_distance_zero_score_km(),
            nearby_distance_bonus_cap: default_nearby_distance_bonus_cap(),
            named_place_full_score_km: default_named_place_full_score_km(),
            named_place_zero_score_km: default_named_place_zero_score_km(),
            named_place_score_weight: default_named_place_score_weight(),
            named_place_distinctive_token_max_place_count:
                default_named_place_distinctive_token_max_place_count(),
            named_place_distinctive_token_max_place_ratio:
                default_named_place_distinctive_token_max_place_ratio(),
            named_place_generic_tokens: default_named_place_generic_tokens(),
            named_place_query_stopwords: default_named_place_query_stopwords(),
            review_rating_weight: default_review_rating_weight(),
            review_count_weight: default_review_count_weight(),
            review_count_log_divisor: default_review_count_log_divisor(),
            min_score_with_positive_evidence: default_min_score_with_positive_evidence(),
            max_score_with_positive_evidence: default_max_score_with_positive_evidence(),
            min_score_with_risk_only_evidence: default_min_score_with_risk_only_evidence(),
            min_score_with_constraint_only: default_min_score_with_constraint_only(),
            fact_coverage_threshold: default_fact_coverage_threshold(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ScoredSignal {
    pub signal_id: String,
    pub score: f64,
    pub availability: FactAvailability,
    pub evidence_count: usize,
    pub missing_fact_keys: Vec<String>,
    pub source_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CandidateScore {
    pub entity_id: String,
    pub property_id: String,
    pub total_score: f64,
    pub signals: Vec<ScoredSignal>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<crate::recommendations::RecallChannelHit>,
}

pub fn scoring_policy_path() -> std::path::PathBuf {
    dag_root().join("scoring_policy.json")
}

pub fn load_scoring_policy() -> Result<ScoringPolicyFile, DagConfigError> {
    let policy: ScoringPolicyFile = load_json(&scoring_policy_path())?;
    validate_policy(&policy)?;
    Ok(policy)
}

pub fn scoring_policy() -> &'static ScoringPolicyFile {
    static POLICY: OnceLock<ScoringPolicyFile> = OnceLock::new();
    POLICY.get_or_init(|| {
        load_scoring_policy().expect("app/config/dag/scoring_policy.json is required")
    })
}

pub fn search_ranking_policy() -> &'static SearchRankingPolicy {
    &scoring_policy().search_ranking
}

pub fn score_property_for_surface(
    property: &Property,
    bundle: Option<&LoadedServingBundle>,
    area_median_ppsf: Option<u64>,
    surface: &str,
) -> CandidateScore {
    let policy = scoring_policy();
    let enabled = enabled_signal_ids(policy, surface);
    let signals = policy
        .signals
        .iter()
        .filter(|signal| enabled.iter().any(|id| id == &signal.id))
        .map(|signal| {
            score_signal(
                property,
                bundle.map(|bundle| &bundle.fact_index),
                area_median_ppsf,
                signal,
            )
        })
        .collect::<Vec<_>>();
    let mut observed_weight = 0.0;
    let weighted_score = signals
        .iter()
        .filter(|signal| signal.availability != FactAvailability::Missing)
        .filter_map(|signal| {
            let weight = policy
                .signals
                .iter()
                .find(|policy_signal| policy_signal.id == signal.signal_id)
                .map(|policy_signal| policy_signal.weight.max(0.0))?;
            observed_weight += weight;
            Some(signal.score * weight)
        })
        .sum::<f64>();
    let total_score = if observed_weight > 0.0 {
        weighted_score / observed_weight
    } else {
        0.0
    };

    CandidateScore {
        entity_id: format!("property:{}", property.id),
        property_id: property.id.clone(),
        total_score: total_score.clamp(0.0, 1.0),
        signals,
        channels: Vec::new(),
    }
}

pub fn signal_score<'a>(
    candidate: &'a CandidateScore,
    signal_id: &str,
) -> Option<&'a ScoredSignal> {
    candidate
        .signals
        .iter()
        .find(|signal| signal.signal_id == signal_id)
}

fn enabled_signal_ids(policy: &ScoringPolicyFile, surface: &str) -> Vec<String> {
    let surface_policy = match surface {
        "detail" => &policy.surfaces.detail,
        "recommendations" => &policy.surfaces.recommendations,
        _ => &policy.surfaces.search,
    };
    if surface_policy.enabled_signals.is_empty() {
        return policy
            .signals
            .iter()
            .map(|signal| signal.id.clone())
            .collect();
    }
    surface_policy.enabled_signals.clone()
}

fn score_signal(
    property: &Property,
    facts: Option<&ServingFactIndex>,
    area_median_ppsf: Option<u64>,
    signal: &ScoringSignalPolicy,
) -> ScoredSignal {
    let rows = facts.and_then(|index| index.entity(&society_node_id(&property.society_id)));
    let matching_facts = rows
        .map(|rows| {
            rows.facts
                .iter()
                .filter(|fact| signal_fact_matches(signal, &fact.fact_key))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let source_types = matching_facts
        .iter()
        .map(|fact| fact.source_type.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let observed = !matching_facts.is_empty()
        || matches!(signal.method, ScoringMethod::LowerIsBetterRelativeToArea)
            && property.price_per_sqft > 0;
    let missing_fact_keys = if observed {
        Vec::new()
    } else {
        signal.fact_keys.clone()
    };
    let availability = if observed {
        FactAvailability::Observed
    } else {
        FactAvailability::Missing
    };
    let score = match signal.method {
        ScoringMethod::LowerIsBetterRelativeToArea => {
            score_price_value(property.price_per_sqft, area_median_ppsf)
        }
        ScoringMethod::EvidenceCoverage => score_evidence_coverage(&matching_facts),
        ScoringMethod::TextOrDistancePresence => score_presence(&matching_facts),
        ScoringMethod::RiskLowerIsBetter => score_risk_lower_is_better(&matching_facts),
    };

    ScoredSignal {
        signal_id: signal.id.clone(),
        score: if observed { score } else { 0.0 },
        availability,
        evidence_count: matching_facts.len(),
        missing_fact_keys,
        source_types,
    }
}

fn signal_fact_matches(signal: &ScoringSignalPolicy, fact_key: &str) -> bool {
    signal
        .fact_keys
        .iter()
        .any(|key| key.eq_ignore_ascii_case(fact_key))
        || signal
            .fact_groups
            .iter()
            .any(|group| fact_belongs_to_group(fact_key, group))
}

fn fact_belongs_to_group(fact_key: &str, group: &str) -> bool {
    if let Some(patterns) = scoring_policy().fact_groups.get(group) {
        return patterns
            .iter()
            .any(|pattern| fact_key_matches_pattern(fact_key, pattern));
    }
    fact_key.starts_with(group)
}

fn fact_key_matches_pattern(fact_key: &str, pattern: &str) -> bool {
    let pattern = pattern.trim();
    if pattern == "*" {
        return true;
    }
    match (pattern.strip_prefix('*'), pattern.strip_suffix('*')) {
        (Some(inner), Some(_)) if pattern.len() >= 2 => {
            fact_key.contains(inner.trim_end_matches('*'))
        }
        (Some(_), Some(_)) => fact_key == pattern,
        (None, Some(prefix)) => fact_key.starts_with(prefix),
        (Some(suffix), None) => fact_key.ends_with(suffix),
        (None, None) => fact_key == pattern,
    }
}

fn score_price_value(price_per_sqft: u64, area_median_ppsf: Option<u64>) -> f64 {
    let Some(median) = area_median_ppsf.filter(|median| *median > 0) else {
        return if price_per_sqft > 0 { 0.5 } else { 0.0 };
    };
    if price_per_sqft == 0 {
        return 0.0;
    }
    let ratio = price_per_sqft as f64 / median as f64;
    if ratio <= 0.85 {
        1.0
    } else if ratio <= 1.0 {
        0.75
    } else if ratio <= 1.12 {
        0.45
    } else {
        0.20
    }
}

fn score_evidence_coverage(facts: &[&ServingFactRecord]) -> f64 {
    if facts.is_empty() {
        return 0.0;
    }
    let source_count = facts
        .iter()
        .map(|fact| fact.source_type.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    let confidence_avg = facts
        .iter()
        .map(|fact| f64::from(fact.confidence))
        .sum::<f64>()
        / facts.len() as f64;
    let coverage = (facts.len() as f64 / scoring_policy().search_ranking.fact_coverage_threshold)
        .clamp(0.0, 1.0);
    let source_diversity = (source_count as f64 / 4.0).clamp(0.0, 1.0);
    (coverage * 0.55 + source_diversity * 0.20 + confidence_avg * 0.25).clamp(0.0, 1.0)
}

fn score_presence(facts: &[&ServingFactRecord]) -> f64 {
    if facts.is_empty() {
        return 0.0;
    }
    let confidence_avg = facts
        .iter()
        .map(|fact| f64::from(fact.confidence))
        .sum::<f64>()
        / facts.len() as f64;
    let breadth = (facts.len() as f64 / 4.0).clamp(0.0, 1.0);
    (breadth * 0.65 + confidence_avg * 0.35).clamp(0.0, 1.0)
}

fn score_risk_lower_is_better(facts: &[&ServingFactRecord]) -> f64 {
    if facts.is_empty() {
        return 0.0;
    }
    let mut scores = Vec::new();
    for fact in facts {
        let score = match &fact.value {
            FactValue::Numeric(value) => (1.0 - (*value).clamp(0.0, 1.0)).clamp(0.0, 1.0),
            FactValue::Bool(value) => {
                if *value {
                    0.25
                } else {
                    0.8
                }
            }
            FactValue::Text(text) => text_safety_score(text),
            FactValue::Tags(tags) => tags
                .iter()
                .map(|tag| text_safety_score(tag))
                .fold(0.0, f64::max),
            FactValue::Score { value, .. } => (1.0 - (*value).clamp(0.0, 1.0)).clamp(0.0, 1.0),
        };
        scores.push(score * f64::from(fact.confidence).clamp(0.0, 1.0));
    }
    (scores.iter().sum::<f64>() / scores.len() as f64).clamp(0.0, 1.0)
}

fn text_safety_score(text: &str) -> f64 {
    let normalized = text.to_ascii_lowercase();
    if [
        "revoked",
        "litigation",
        "complaint",
        "delay",
        "delayed",
        "risk",
        "issue",
    ]
    .iter()
    .any(|term| normalized.contains(term))
    {
        0.25
    } else if [
        "clear",
        "approved",
        "delivered",
        "completed",
        "none",
        "zero",
    ]
    .iter()
    .any(|term| normalized.contains(term))
    {
        0.85
    } else {
        0.55
    }
}

fn validate_policy(policy: &ScoringPolicyFile) -> Result<(), DagConfigError> {
    validate_recommendation_recall_policy(&policy.recommendation_recall)?;

    let signal_ids = policy
        .signals
        .iter()
        .map(|signal| signal.id.as_str())
        .collect::<BTreeSet<_>>();
    for branch in &policy.recommendation_branches {
        if !signal_ids.contains(branch.primary_signal.as_str()) {
            return Err(DagConfigError::InvalidConfig(format!(
                "recommendation branch {} references missing signal {}",
                branch.id, branch.primary_signal
            )));
        }
        validate_recommendation_lens(
            &branch.lens,
            &format!("recommendation branch {}", branch.id),
        )?;
    }

    let registry = load_fact_registry_index()?;
    let runtime_fact_keys = policy
        .runtime_fact_keys
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for signal in &policy.signals {
        if signal.weight < 0.0 || signal.weight > 5.0 {
            return Err(DagConfigError::InvalidConfig(format!(
                "signal {} weight {} is outside 0..5",
                signal.id, signal.weight
            )));
        }
        for fact_key in &signal.fact_keys {
            if registry.lookup(fact_key).is_none() && !runtime_fact_keys.contains(fact_key.as_str())
            {
                return Err(DagConfigError::InvalidConfig(format!(
                    "signal {} references unknown fact_key {}",
                    signal.id, fact_key
                )));
            }
        }
    }
    Ok(())
}

fn validate_recommendation_recall_policy(
    policy: &RecommendationRecallPolicy,
) -> Result<(), DagConfigError> {
    if policy.candidate_limit == 0 {
        return Err(DagConfigError::InvalidConfig(
            "recommendation_recall candidate_limit must be greater than zero".to_string(),
        ));
    }
    if policy.branch_limit == 0 {
        return Err(DagConfigError::InvalidConfig(
            "recommendation_recall branch_limit must be greater than zero".to_string(),
        ));
    }
    if policy.target_branch_count > policy.branch_limit {
        return Err(DagConfigError::InvalidConfig(
            "recommendation_recall target_branch_count cannot exceed branch_limit".to_string(),
        ));
    }

    let mut channel_ids = BTreeSet::new();
    for channel in &policy.channels {
        if !channel_ids.insert(channel.id.as_str()) {
            return Err(DagConfigError::InvalidConfig(format!(
                "recommendation recall channel {} is duplicated",
                channel.id
            )));
        }
        if !matches!(
            channel.kind.as_str(),
            "same_area_bhk"
                | "area_alias_bhk"
                | "price_band"
                | "builder_family"
                | "serving_graph"
                | "tantivy_lexical"
        ) {
            return Err(DagConfigError::InvalidConfig(format!(
                "recommendation recall channel {} has unsupported kind {}",
                channel.id, channel.kind
            )));
        }
        if !(0.0..=1.0).contains(&channel.score) {
            return Err(DagConfigError::InvalidConfig(format!(
                "recommendation recall channel {} score {} is outside 0..1",
                channel.id, channel.score
            )));
        }
        if channel.limit.is_some_and(|limit| limit == 0) {
            return Err(DagConfigError::InvalidConfig(format!(
                "recommendation recall channel {} limit must be greater than zero",
                channel.id
            )));
        }
    }

    if policy.fallback_branch.max_items == 0 {
        return Err(DagConfigError::InvalidConfig(
            "recommendation fallback branch max_items must be greater than zero".to_string(),
        ));
    }
    validate_recommendation_lens(
        &policy.fallback_branch.lens,
        "recommendation fallback branch",
    )?;

    for tie_breaker in &policy.tie_breakers {
        if !matches!(
            tie_breaker.as_str(),
            "review_strength_desc" | "magnitude_desc" | "branch_priority_asc"
        ) {
            return Err(DagConfigError::InvalidConfig(format!(
                "recommendation tie breaker {tie_breaker} is unsupported"
            )));
        }
    }

    Ok(())
}

fn validate_recommendation_lens(lens: &str, context: &str) -> Result<(), DagConfigError> {
    if matches!(lens, "proof" | "value" | "trust" | "commute") {
        Ok(())
    } else {
        Err(DagConfigError::InvalidConfig(format!(
            "{context} has unsupported lens {lens}"
        )))
    }
}

fn default_true() -> bool {
    true
}

fn default_missing_behavior() -> MissingDataBehavior {
    MissingDataBehavior::Skip
}

fn default_signal_weight() -> f64 {
    1.0
}

fn default_min_support_evidence_confidence() -> f32 {
    0.60
}
fn default_min_llm_evidence_confidence() -> f32 {
    0.75
}
fn default_negative_no_data_penalty_multiplier() -> f64 {
    1.2
}
fn default_min_semantic_recall_score() -> f64 {
    0.08
}
fn default_semantic_candidate_fit_weight() -> f64 {
    1.0
}
fn default_semantic_candidate_fit_cap() -> f64 {
    0.25
}
fn default_broad_local_recall_multiplier() -> usize {
    4
}
fn default_broad_local_recall_min_extra() -> usize {
    64
}
fn default_positive_evidence_floor_ratio() -> f64 {
    0.60
}
fn default_no_positive_evidence_score_multiplier() -> f64 {
    0.40
}
fn default_nearby_area_score_penalty() -> f64 {
    -0.35
}
fn default_graph_area_score_penalty() -> f64 {
    -0.25
}
fn default_nearby_distance_full_score_km() -> f64 {
    0.75
}
fn default_nearby_distance_zero_score_km() -> f64 {
    3.0
}
fn default_nearby_distance_bonus_cap() -> f64 {
    0.8
}
fn default_named_place_full_score_km() -> f64 {
    0.75
}
fn default_named_place_zero_score_km() -> f64 {
    5.0
}
fn default_named_place_score_weight() -> f64 {
    2.0
}
fn default_named_place_distinctive_token_max_place_count() -> usize {
    3
}
fn default_named_place_distinctive_token_max_place_ratio() -> f64 {
    0.15
}
fn default_named_place_generic_tokens() -> Vec<String> {
    [
        "the",
        "and",
        "school",
        "academy",
        "hospital",
        "metro",
        "station",
        "park",
        "road",
        "tech",
        "technology",
        "business",
        "office",
        "bengaluru",
        "bangalore",
        "whitefield",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}
fn default_named_place_query_stopwords() -> Vec<String> {
    [
        "near",
        "a",
        "an",
        "in",
        "at",
        "from",
        "to",
        "with",
        "within",
        "km",
        "kms",
        "kilometer",
        "kilometers",
        "m",
        "meter",
        "meters",
        "metre",
        "metres",
        "bhk",
        "flat",
        "apartment",
        "home",
        "homes",
        "property",
        "properties",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}
fn default_review_rating_weight() -> f64 {
    0.75
}
fn default_review_count_weight() -> f64 {
    0.25
}
fn default_review_count_log_divisor() -> f64 {
    8.0
}
fn default_min_score_with_positive_evidence() -> f64 {
    0.2
}
fn default_max_score_with_positive_evidence() -> f64 {
    0.45
}
fn default_min_score_with_risk_only_evidence() -> f64 {
    0.1
}
fn default_min_score_with_constraint_only() -> f64 {
    0.01
}
fn default_fact_coverage_threshold() -> f64 {
    25.0
}
fn default_recommendation_candidate_limit() -> usize {
    80
}
fn default_recommendation_branch_limit() -> usize {
    6
}
fn default_recommendation_target_branch_count() -> usize {
    3
}
fn default_recommendation_channel_score() -> f64 {
    1.0
}
fn default_recommendation_fallback_id() -> String {
    "fallback".to_string()
}
fn default_recommendation_fallback_headline() -> String {
    "Fallback".to_string()
}
fn default_recommendation_fallback_lens() -> String {
    "proof".to_string()
}
fn default_recommendation_fallback_max_items() -> usize {
    3
}
fn default_recommendation_tie_breakers() -> Vec<String> {
    ["review_strength_desc", "magnitude_desc"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoring_policy_loads_and_validates() {
        let policy = load_scoring_policy().expect("scoring policy should load");
        assert!(!policy.signals.is_empty());
        assert!(!policy.recommendation_branches.is_empty());
        assert!(!policy.recommendation_recall.channels.is_empty());
        assert!(policy.missing_data.never_zero_fill);
    }

    #[test]
    fn missing_price_does_not_score_as_good_value() {
        assert_eq!(score_price_value(0, Some(10_000)), 0.0);
    }

    #[test]
    fn recommendation_recall_policy_validates_channel_metadata() {
        let mut policy = ScoringPolicyFile {
            version: 1,
            engine_version: "test".to_string(),
            missing_data: MissingDataPolicy::default(),
            search_ranking: SearchRankingPolicy::default(),
            fact_groups: BTreeMap::new(),
            runtime_fact_keys: Vec::new(),
            signals: Vec::new(),
            surfaces: ScoringSurfaces::default(),
            recommendation_recall: RecommendationRecallPolicy::default(),
            recommendation_branches: Vec::new(),
        };
        policy
            .recommendation_recall
            .channels
            .push(RecommendationRecallChannelPolicy {
                id: "bad_channel".to_string(),
                kind: "hardcoded_magic".to_string(),
                enabled: true,
                score: 1.0,
                limit: None,
                edge_types: Vec::new(),
            });

        assert!(validate_policy(&policy).is_err());
    }
}
