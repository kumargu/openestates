use std::collections::{BTreeSet, HashMap, HashSet};

use crate::knowledge::graph::KnowledgeGraph;
use crate::models::{Property, PropertyCard, Society};
use crate::routes::enrichment::{enrich_property_card, society_node_id};
use crate::routes::properties::{
    build_source_panels, evidence_section_from_panel, overlay_serving_google_reviews,
    PropertyEvidenceResponse,
};
use crate::scoring::{
    score_property_for_surface, scoring_policy, signal_score, CandidateScore, FactAvailability,
    RecommendationBranchPolicy, RecommendationEligibilityPolicy,
    RecommendationFallbackBranchPolicy, RecommendationRecallChannelPolicy,
    RecommendationRecallOperator, RecommendationRecallPolicy, ScoredSignal,
};
use crate::serving::{unique_society_aliases, LoadedServingBundle, TantivyRecallHit};

use super::branch::{
    compass_magnitude, BranchLens, EvidenceDelta, RecallChannelHit, RecommendationBranch,
};
use super::snapshot::{summarize_evidence_sections, EvidenceSnapshot};

const RECOMMENDATION_SURFACE: &str = "recommendations";

struct Candidate {
    property: Property,
    card: PropertyCard,
    snapshot: EvidenceSnapshot,
    score: CandidateScore,
    channels: Vec<RecallChannelHit>,
}

pub struct RecommendationBranchInputs<'a> {
    pub current: &'a Property,
    pub current_evidence: &'a PropertyEvidenceResponse,
    pub graph: &'a KnowledgeGraph,
    pub properties: &'a [Property],
    pub societies: &'a [Society],
    pub serving_bundle: Option<&'a LoadedServingBundle>,
    pub area_median_ppsf: Option<u64>,
}

pub fn build_recommendation_branches(
    inputs: RecommendationBranchInputs<'_>,
) -> Vec<RecommendationBranch> {
    let RecommendationBranchInputs {
        current,
        current_evidence,
        graph,
        properties,
        societies,
        serving_bundle,
        area_median_ppsf,
    } = inputs;

    let current_snapshot = summarize_evidence_sections(&current_evidence.sections);
    let current_score = score_property_for_surface(
        current,
        serving_bundle,
        area_median_ppsf,
        RECOMMENDATION_SURFACE,
    );
    let policy = scoring_policy();
    let recall_policy = &policy.recommendation_recall;
    let candidates = recall_candidates(
        current,
        graph,
        properties,
        societies,
        serving_bundle,
        area_median_ppsf,
        recall_policy,
    );
    if candidates.is_empty() {
        return Vec::new();
    }

    let mut branches = Vec::new();
    let mut used_ids = HashSet::new();
    let mut society_counts = HashMap::<String, usize>::new();
    if recall_policy.eligibility.exclude_anchor_society {
        society_counts.insert(
            society_key(current),
            recall_policy.eligibility.max_properties_per_society,
        );
    }
    for branch_policy in ordered_branch_policies(&policy.recommendation_branches) {
        if let Some(branch) = pick_policy_branch(
            branch_policy,
            current,
            current_snapshot,
            &current_score,
            &candidates,
            &used_ids,
            &society_counts,
            &recall_policy.eligibility,
        ) {
            used_ids.insert(branch.property.id.clone());
            *society_counts
                .entry(society_key_from_card(&branch.property))
                .or_default() += 1;
            branches.push(branch);
        }
    }

    if recall_policy.fallback_branch.enabled && branches.len() < recall_policy.target_branch_count {
        fill_with_similar_tradeoffs(
            current,
            current_snapshot,
            &current_score,
            &candidates,
            &mut used_ids,
            &mut society_counts,
            &mut branches,
            &recall_policy.fallback_branch,
            &recall_policy.eligibility,
            recall_policy.target_branch_count,
        );
    }

    branches.truncate(recall_policy.branch_limit);
    branches
}

fn recall_candidates(
    current: &Property,
    graph: &KnowledgeGraph,
    properties: &[Property],
    societies: &[Society],
    serving_bundle: Option<&LoadedServingBundle>,
    area_median_ppsf: Option<u64>,
    recall_policy: &RecommendationRecallPolicy,
) -> Vec<Candidate> {
    let mut channels_by_id = HashMap::<String, Vec<RecallChannelHit>>::new();
    let mut channel_counts = HashMap::<String, usize>::new();
    let eligible_ids = properties
        .iter()
        .filter(|property| {
            property.id != current.id
                && property.is_listable()
                && recommendation_compatible(current, property, &recall_policy.eligibility)
        })
        .map(|property| property.id.as_str())
        .collect::<HashSet<_>>();

    for property in properties {
        if !eligible_ids.contains(property.id.as_str()) {
            continue;
        }
        for channel in recall_policy
            .channels
            .iter()
            .filter(|channel| channel.enabled)
        {
            let matched = match channel.operator {
                RecommendationRecallOperator::SameArea => same_area(current, property),
                RecommendationRecallOperator::PriceBand => {
                    same_price_band(current, property, channel)
                }
                RecommendationRecallOperator::SameBuilder => same_builder(current, property),
                RecommendationRecallOperator::SharedGraphNeighbor
                | RecommendationRecallOperator::SpatialRadius
                | RecommendationRecallOperator::Lexical => false,
            };
            if matched {
                add_configured_channel(
                    &mut channels_by_id,
                    &mut channel_counts,
                    &property.id,
                    channel,
                    channel.score,
                );
            }
        }
    }

    let property_index = PropertyEntityIndex::new(properties, serving_bundle, &eligible_ids);
    add_serving_graph_recall(
        current,
        serving_bundle,
        recall_policy,
        &property_index,
        &mut channels_by_id,
        &mut channel_counts,
    );
    add_spatial_recall(
        current,
        serving_bundle,
        recall_policy,
        &property_index,
        &mut channels_by_id,
        &mut channel_counts,
    );
    add_tantivy_recall(
        current,
        properties,
        serving_bundle,
        recall_policy,
        &property_index,
        &mut channels_by_id,
        &mut channel_counts,
    );

    let recall_channel_ids = recall_policy
        .channels
        .iter()
        .filter(|channel| channel.enabled && channel.can_recall)
        .map(|channel| channel.id.as_str())
        .collect::<HashSet<_>>();
    channels_by_id.retain(|_, channels| {
        channels
            .iter()
            .any(|channel| recall_channel_ids.contains(channel.channel.as_str()))
    });

    let mut ordered = channels_by_id.into_iter().collect::<Vec<_>>();
    ordered.sort_by(|(left_id, left_channels), (right_id, right_channels)| {
        channel_strength(right_channels)
            .partial_cmp(&channel_strength(left_channels))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left_id.cmp(right_id))
    });

    ordered
        .into_iter()
        .take(recall_policy.candidate_limit)
        .filter_map(|(id, mut channels)| {
            let property = properties.iter().find(|property| property.id == id)?;
            channels.sort_by(|left, right| left.channel.cmp(&right.channel));
            let card = overlay_serving_google_reviews(
                enrich_property_card(property, societies, graph),
                &property.society_id,
                serving_bundle.map(|bundle| &bundle.fact_index),
            );
            let source_panels = build_source_panels(
                graph,
                property,
                serving_bundle.map(|bundle| &bundle.fact_index),
                serving_bundle.map(|bundle| &bundle.graph_index),
            );
            let sections = source_panels
                .into_iter()
                .map(|panel| evidence_section_from_panel(panel, &card.kg_entity_refs))
                .collect::<Vec<_>>();
            let snapshot = summarize_evidence_sections(&sections);
            let score = score_property_for_surface(
                property,
                serving_bundle,
                area_median_ppsf,
                RECOMMENDATION_SURFACE,
            );
            Some(Candidate {
                property: property.clone(),
                card,
                snapshot,
                score,
                channels,
            })
        })
        .collect()
}

struct PropertyEntityIndex {
    entity_by_property: HashMap<String, String>,
    properties_by_entity: HashMap<String, Vec<String>>,
}

impl PropertyEntityIndex {
    fn new(
        properties: &[Property],
        serving_bundle: Option<&LoadedServingBundle>,
        eligible_ids: &HashSet<&str>,
    ) -> Self {
        let canonical_by_alias: HashMap<String, String> = serving_bundle
            .map(|bundle| {
                unique_society_aliases(&bundle.entities)
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default();
        let mut entity_by_property = HashMap::new();
        let mut properties_by_entity = HashMap::<String, Vec<String>>::new();
        for property in properties {
            let alias = society_node_id(&property.society_id);
            let entity_id = canonical_by_alias.get(&alias).cloned().unwrap_or(alias);
            entity_by_property.insert(property.id.clone(), entity_id.clone());
            if eligible_ids.contains(property.id.as_str()) {
                properties_by_entity
                    .entry(entity_id)
                    .or_default()
                    .push(property.id.clone());
            }
        }
        for property_ids in properties_by_entity.values_mut() {
            property_ids.sort();
        }
        Self {
            entity_by_property,
            properties_by_entity,
        }
    }

    fn entity_for_property(&self, property_id: &str) -> Option<&str> {
        self.entity_by_property.get(property_id).map(String::as_str)
    }

    fn property_ids_for_entity(&self, entity_id: &str) -> &[String] {
        self.properties_by_entity
            .get(entity_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

fn add_serving_graph_recall(
    current: &Property,
    serving_bundle: Option<&LoadedServingBundle>,
    recall_policy: &RecommendationRecallPolicy,
    property_index: &PropertyEntityIndex,
    channels_by_id: &mut HashMap<String, Vec<RecallChannelHit>>,
    channel_counts: &mut HashMap<String, usize>,
) {
    let Some(bundle) = serving_bundle else {
        return;
    };
    let channels = channels_for_operator(
        recall_policy,
        RecommendationRecallOperator::SharedGraphNeighbor,
    );
    let Some(current_society) = property_index.entity_for_property(&current.id) else {
        return;
    };

    for channel in channels {
        let current_neighbors = bundle
            .edges
            .iter()
            .filter(|edge| edge.from_entity_id == current_society)
            .filter(|edge| {
                channel
                    .edge_types
                    .iter()
                    .any(|kind| kind == &edge.edge_type)
            })
            .map(|edge| (edge.edge_type.as_str(), edge.to_entity_id.as_str()))
            .collect::<BTreeSet<_>>();
        if current_neighbors.is_empty() {
            continue;
        }
        for edge in &bundle.edges {
            if edge.from_entity_id == current_society
                || !current_neighbors
                    .contains(&(edge.edge_type.as_str(), edge.to_entity_id.as_str()))
            {
                continue;
            }
            for property_id in property_index.property_ids_for_entity(&edge.from_entity_id) {
                add_configured_channel(
                    channels_by_id,
                    channel_counts,
                    property_id,
                    channel,
                    channel.score,
                );
            }
        }
    }
}

fn add_spatial_recall(
    current: &Property,
    serving_bundle: Option<&LoadedServingBundle>,
    recall_policy: &RecommendationRecallPolicy,
    property_index: &PropertyEntityIndex,
    channels_by_id: &mut HashMap<String, Vec<RecallChannelHit>>,
    channel_counts: &mut HashMap<String, usize>,
) {
    let Some(bundle) = serving_bundle else {
        return;
    };
    let Some(current_society) = property_index.entity_for_property(&current.id) else {
        return;
    };
    let Some(anchor) = bundle.spatial_index.point_for_entity(current_society) else {
        return;
    };
    for channel in channels_for_operator(recall_policy, RecommendationRecallOperator::SpatialRadius)
    {
        let Some(max_distance_km) = channel.max_distance_km else {
            continue;
        };
        let limit = channel.limit.unwrap_or(recall_policy.candidate_limit);
        for (point, distance_km) in bundle.spatial_index.nearest_societies(
            anchor.latitude,
            anchor.longitude,
            limit.saturating_add(1),
        ) {
            if point.entity_id == current_society || distance_km > max_distance_km {
                continue;
            }
            let proximity = (1.0 - distance_km / max_distance_km).clamp(0.0, 1.0);
            if proximity <= 0.0 {
                continue;
            }
            for property_id in property_index.property_ids_for_entity(&point.entity_id) {
                add_configured_channel(
                    channels_by_id,
                    channel_counts,
                    property_id,
                    channel,
                    channel.score * proximity,
                );
            }
        }
    }
}

fn add_tantivy_recall(
    current: &Property,
    properties: &[Property],
    serving_bundle: Option<&LoadedServingBundle>,
    recall_policy: &RecommendationRecallPolicy,
    property_index: &PropertyEntityIndex,
    channels_by_id: &mut HashMap<String, Vec<RecallChannelHit>>,
    channel_counts: &mut HashMap<String, usize>,
) {
    let Some(bundle) = serving_bundle else {
        return;
    };
    let channels = channels_for_operator(recall_policy, RecommendationRecallOperator::Lexical);
    if channels.is_empty() {
        return;
    }
    let search_limit = channels
        .iter()
        .filter_map(|channel| channel.limit)
        .max()
        .unwrap_or(recall_policy.candidate_limit);
    let query = recommendation_query(current);
    let Ok(hits) = bundle.recall_index.search(&query, search_limit) else {
        return;
    };
    let max_score = hits
        .iter()
        .map(|hit| hit.score.max(0.0))
        .fold(0.0_f32, f32::max)
        .max(1.0);

    for hit in hits {
        let score = f64::from(hit.score.max(0.0) / max_score);
        for property_id in property_ids_for_tantivy_hit(&hit, properties, property_index) {
            if property_id != current.id {
                for channel in &channels {
                    add_configured_channel(
                        channels_by_id,
                        channel_counts,
                        &property_id,
                        channel,
                        score * channel.score,
                    );
                }
            }
        }
    }
}

fn property_ids_for_tantivy_hit(
    hit: &TantivyRecallHit,
    properties: &[Property],
    property_index: &PropertyEntityIndex,
) -> Vec<String> {
    if let Some(id) = hit.entity_id.strip_prefix("property:") {
        return if properties.iter().any(|property| property.id == id) {
            vec![id.to_string()]
        } else {
            Vec::new()
        };
    }
    if hit.entity_id.starts_with("society:") {
        return property_index
            .property_ids_for_entity(&hit.entity_id)
            .to_vec();
    }
    Vec::new()
}

fn pick_policy_branch(
    branch_policy: &RecommendationBranchPolicy,
    current: &Property,
    current_snapshot: EvidenceSnapshot,
    current_score: &CandidateScore,
    candidates: &[Candidate],
    used_ids: &HashSet<String>,
    society_counts: &HashMap<String, usize>,
    eligibility: &RecommendationEligibilityPolicy,
) -> Option<RecommendationBranch> {
    let current_signal = signal_score(current_score, &branch_policy.primary_signal);
    let best = candidates
        .iter()
        .filter(|candidate| !used_ids.contains(&candidate.property.id))
        .filter(|candidate| candidate_society_available(candidate, society_counts, eligibility))
        .filter_map(|candidate| {
            let candidate_signal = signal_score(&candidate.score, &branch_policy.primary_signal)?;
            if candidate_signal.availability != FactAvailability::Observed {
                return None;
            }
            let current_value = current_signal
                .filter(|signal| signal.availability != FactAvailability::Missing)
                .map(|signal| signal.score)
                .unwrap_or(0.0);
            let delta = candidate_signal.score - current_value;
            (delta >= branch_policy.min_delta).then_some((candidate, candidate_signal, delta))
        })
        .max_by(|left, right| {
            left.2
                .partial_cmp(&right.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    channel_strength(&left.0.channels)
                        .partial_cmp(&channel_strength(&right.0.channels))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    left.0
                        .score
                        .total_score
                        .partial_cmp(&right.0.score.total_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| right.0.property.id.cmp(&left.0.property.id))
        })?;

    let (candidate, candidate_signal, delta) = best;
    Some(RecommendationBranch {
        branch_id: branch_policy.id.clone(),
        lens: BranchLens::from_policy_lens(&branch_policy.lens),
        headline: branch_policy.headline.clone(),
        property: candidate.card.clone(),
        contrast: contrast_for_signal(
            &branch_policy.primary_signal,
            current,
            &candidate.property,
            current_signal,
            candidate_signal,
        ),
        tradeoff: tradeoff_for_candidate(current, current_score, candidate),
        evidence_delta: delta_from_snapshots(current_snapshot, candidate.snapshot),
        channels: candidate.channels.clone(),
        magnitude: compass_magnitude((delta / branch_policy.min_delta.max(0.01) / 2.0) as f32),
    })
}

fn fill_with_similar_tradeoffs(
    current: &Property,
    current_snapshot: EvidenceSnapshot,
    current_score: &CandidateScore,
    candidates: &[Candidate],
    used_ids: &mut HashSet<String>,
    society_counts: &mut HashMap<String, usize>,
    branches: &mut Vec<RecommendationBranch>,
    fallback_policy: &RecommendationFallbackBranchPolicy,
    eligibility: &RecommendationEligibilityPolicy,
    target_branch_count: usize,
) {
    let mut remaining = candidates
        .iter()
        .filter(|candidate| !used_ids.contains(&candidate.property.id))
        .filter(|candidate| candidate_society_available(candidate, society_counts, eligibility))
        .collect::<Vec<_>>();
    remaining.sort_by(|left, right| {
        channel_strength(&right.channels)
            .partial_cmp(&channel_strength(&left.channels))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .score
                    .total_score
                    .partial_cmp(&left.score.total_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.property.id.cmp(&right.property.id))
    });

    for candidate in remaining.into_iter().take(fallback_policy.max_items) {
        if !candidate_society_available(candidate, society_counts, eligibility) {
            continue;
        }
        used_ids.insert(candidate.property.id.clone());
        *society_counts
            .entry(society_key(&candidate.property))
            .or_default() += 1;
        branches.push(RecommendationBranch {
            branch_id: fallback_policy.id.clone(),
            lens: BranchLens::from_policy_lens(&fallback_policy.lens),
            headline: fallback_policy.headline.clone(),
            property: candidate.card.clone(),
            contrast: best_available_contrast(current, current_score, candidate),
            tradeoff: tradeoff_for_candidate(current, current_score, candidate),
            evidence_delta: delta_from_snapshots(current_snapshot, candidate.snapshot),
            channels: candidate.channels.clone(),
            magnitude: compass_magnitude(channel_strength(&candidate.channels) as f32 / 2.0),
        });
        if branches.len() >= target_branch_count {
            break;
        }
    }
}

fn contrast_for_signal(
    signal_id: &str,
    current: &Property,
    candidate: &Property,
    current_signal: Option<&ScoredSignal>,
    candidate_signal: &ScoredSignal,
) -> String {
    if signal_id == "price_value" && current.price_per_sqft > 0 && candidate.price_per_sqft > 0 {
        let pct = current
            .price_per_sqft
            .saturating_sub(candidate.price_per_sqft) as f64
            / current.price_per_sqft as f64
            * 100.0;
        if pct > 0.0 {
            return format!("{:.0}% lower per sqft in {}", pct.round(), candidate.area);
        }
    }

    let current_count = current_signal
        .map(|signal| signal.evidence_count)
        .unwrap_or(0);
    if candidate_signal.evidence_count != current_count {
        return format!(
            "{} receipts vs {} here for {}",
            candidate_signal.evidence_count,
            current_count,
            signal_label(signal_id)
        );
    }
    format!("Stronger {} evidence", signal_label(signal_id))
}

fn best_available_contrast(
    current: &Property,
    current_score: &CandidateScore,
    candidate: &Candidate,
) -> String {
    let mut best_signal = None;
    for candidate_signal in &candidate.score.signals {
        if candidate_signal.availability != FactAvailability::Observed {
            continue;
        }
        let current_value = signal_score(current_score, &candidate_signal.signal_id)
            .filter(|signal| signal.availability != FactAvailability::Missing)
            .map(|signal| signal.score)
            .unwrap_or(0.0);
        let delta = candidate_signal.score - current_value;
        if delta > best_signal.map(|(_, best_delta)| best_delta).unwrap_or(0.0) {
            best_signal = Some((candidate_signal, delta));
        }
    }

    if let Some((signal, _)) = best_signal {
        return contrast_for_signal(
            &signal.signal_id,
            current,
            &candidate.property,
            signal_score(current_score, &signal.signal_id),
            signal,
        );
    }
    "Similar profile from the serving bundle".to_string()
}

fn tradeoff_for_candidate(
    current: &Property,
    current_score: &CandidateScore,
    candidate: &Candidate,
) -> Option<String> {
    if current.price_per_sqft > 0 && candidate.property.price_per_sqft > current.price_per_sqft {
        let pct = ((candidate.property.price_per_sqft - current.price_per_sqft) as f64
            / current.price_per_sqft as f64
            * 100.0)
            .round() as u64;
        if pct >= 4 {
            return Some(format!("Tradeoff: ~{pct}% higher per sqft"));
        }
    }

    let current_proof = signal_score(current_score, "proof_strength")?;
    let candidate_proof = signal_score(&candidate.score, "proof_strength")?;
    if current_proof.availability == FactAvailability::Observed
        && candidate_proof.availability == FactAvailability::Observed
        && candidate_proof.score + 0.12 < current_proof.score
    {
        return Some("Tradeoff: weaker proof coverage".to_string());
    }
    None
}

fn delta_from_snapshots(current: EvidenceSnapshot, candidate: EvidenceSnapshot) -> EvidenceDelta {
    EvidenceDelta {
        fact_count: candidate.fact_count,
        gap_count: candidate.gap_count,
        confidence_pct: candidate.confidence_pct,
        fact_delta: candidate.fact_count as i32 - current.fact_count as i32,
        gap_delta: candidate.gap_count as i32 - current.gap_count as i32,
    }
}

fn add_channel(
    channels_by_id: &mut HashMap<String, Vec<RecallChannelHit>>,
    property_id: &str,
    channel: &str,
    score: f64,
) {
    let channels = channels_by_id.entry(property_id.to_string()).or_default();
    if let Some(existing) = channels.iter_mut().find(|hit| hit.channel == channel) {
        existing.score = existing.score.max(score);
        return;
    }
    channels.push(RecallChannelHit {
        channel: channel.to_string(),
        score: score.clamp(0.0, 1.0),
    });
}

fn add_configured_channel(
    channels_by_id: &mut HashMap<String, Vec<RecallChannelHit>>,
    channel_counts: &mut HashMap<String, usize>,
    property_id: &str,
    channel: &RecommendationRecallChannelPolicy,
    score: f64,
) {
    let already_present = channels_by_id
        .get(property_id)
        .is_some_and(|channels| channels.iter().any(|hit| hit.channel == channel.id));
    if !already_present
        && channel
            .limit
            .is_some_and(|limit| channel_counts.get(&channel.id).copied().unwrap_or(0) >= limit)
    {
        return;
    }
    add_channel(channels_by_id, property_id, &channel.id, score);
    if !already_present {
        *channel_counts.entry(channel.id.clone()).or_default() += 1;
    }
}

fn channels_for_operator(
    policy: &RecommendationRecallPolicy,
    operator: RecommendationRecallOperator,
) -> Vec<&RecommendationRecallChannelPolicy> {
    policy
        .channels
        .iter()
        .filter(|channel| channel.enabled && channel.operator == operator)
        .collect()
}

fn ordered_branch_policies(
    policies: &[RecommendationBranchPolicy],
) -> Vec<&RecommendationBranchPolicy> {
    let mut ordered = policies.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.id.cmp(&right.id))
    });
    ordered
}

fn channel_strength(channels: &[RecallChannelHit]) -> f64 {
    channels.iter().map(|channel| channel.score).sum::<f64>()
}

fn same_area(current: &Property, candidate: &Property) -> bool {
    normalize(&current.area_id) == normalize(&candidate.area_id)
        || normalize(&current.area) == normalize(&candidate.area)
}

fn same_price_band(
    current: &Property,
    candidate: &Property,
    channel: &RecommendationRecallChannelPolicy,
) -> bool {
    if current.price_per_sqft == 0 || candidate.price_per_sqft == 0 {
        return false;
    }
    let ratio = candidate.price_per_sqft as f64 / current.price_per_sqft as f64;
    ratio >= channel.min_ratio.unwrap_or(1.0) && ratio <= channel.max_ratio.unwrap_or(1.0)
}

fn same_builder(current: &Property, candidate: &Property) -> bool {
    let current_builder = normalize(&current.builder_name);
    !current_builder.is_empty() && current_builder == normalize(&candidate.builder_name)
}

fn recommendation_compatible(
    current: &Property,
    candidate: &Property,
    policy: &RecommendationEligibilityPolicy,
) -> bool {
    (!policy.require_same_bhk || current.bhk == candidate.bhk)
        && (!policy.require_same_listing_type
            || normalize(&current.listing_type) == normalize(&candidate.listing_type))
        && (!policy.require_same_property_type
            || compatible_property_type(current, candidate, policy))
}

fn compatible_property_type(
    current: &Property,
    candidate: &Property,
    policy: &RecommendationEligibilityPolicy,
) -> bool {
    let current_type = normalize(&current.property_type);
    let candidate_type = normalize(&candidate.property_type);
    current_type == candidate_type
        || policy.compatible_property_type_groups.iter().any(|group| {
            group
                .values
                .iter()
                .map(|value| normalize(value))
                .any(|value| value == current_type)
                && group
                    .values
                    .iter()
                    .map(|value| normalize(value))
                    .any(|value| value == candidate_type)
        })
}

fn candidate_society_available(
    candidate: &Candidate,
    society_counts: &HashMap<String, usize>,
    policy: &RecommendationEligibilityPolicy,
) -> bool {
    society_counts
        .get(&society_key(&candidate.property))
        .copied()
        .unwrap_or(0)
        < policy.max_properties_per_society
}

fn society_key(property: &Property) -> String {
    society_node_id(&property.society_id)
}

fn society_key_from_card(property: &PropertyCard) -> String {
    property.kg_entity_refs.society_entity_id.clone()
}

fn recommendation_query(current: &Property) -> String {
    format!(
        "{} {} {} {} {}",
        current.title,
        current.area,
        current.society_id,
        current.builder_name,
        current.transparency_tags.join(" ")
    )
}

fn signal_label(signal_id: &str) -> String {
    signal_id.replace('_', " ")
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}
