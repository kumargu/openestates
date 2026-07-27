use std::collections::{BTreeSet, HashMap, HashSet};

use crate::dag_config::area_alias_entries;
use crate::knowledge::graph::KnowledgeGraph;
use crate::models::{Property, PropertyCard, Society};
use crate::routes::enrichment::{enrich_property_card, society_node_id};
use crate::routes::properties::{
    build_source_panels, evidence_section_from_panel, overlay_serving_google_reviews,
    PropertyEvidenceResponse,
};
use crate::scoring::{
    score_property_for_surface, scoring_policy, signal_score, CandidateScore, FactAvailability,
    RecommendationBranchPolicy, RecommendationFallbackBranchPolicy,
    RecommendationRecallChannelPolicy, RecommendationRecallPolicy, ScoredSignal,
};
use crate::serving::{LoadedServingBundle, TantivyRecallHit};

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
    for branch_policy in ordered_branch_policies(&policy.recommendation_branches) {
        if let Some(branch) = pick_policy_branch(
            branch_policy,
            current,
            current_snapshot,
            &current_score,
            &candidates,
            &used_ids,
        ) {
            used_ids.insert(branch.property.id.clone());
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
            &mut branches,
            &recall_policy.fallback_branch,
            recall_policy.target_branch_count,
        );
    }

    sort_branches(
        &mut branches,
        recall_policy,
        &policy.recommendation_branches,
    );
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

    for property in properties {
        if property.id == current.id || !property.is_listable() {
            continue;
        }
        for channel in recall_policy
            .channels
            .iter()
            .filter(|channel| channel.enabled)
        {
            let matched = match channel.kind.as_str() {
                "same_area_bhk" => property.bhk == current.bhk && same_area(current, property),
                "area_alias_bhk" => {
                    property.bhk == current.bhk && alias_area_match(&current.area, &property.area)
                }
                "price_band" => same_price_band(current, property),
                "builder_family" => same_builder_family(current, property),
                _ => false,
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

    add_serving_graph_recall(
        current,
        properties,
        serving_bundle,
        recall_policy,
        &mut channels_by_id,
        &mut channel_counts,
    );
    add_tantivy_recall(
        current,
        properties,
        serving_bundle,
        recall_policy,
        &mut channels_by_id,
        &mut channel_counts,
    );

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
        .filter_map(|(id, channels)| {
            let property = properties.iter().find(|property| property.id == id)?;
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

fn add_serving_graph_recall(
    current: &Property,
    properties: &[Property],
    serving_bundle: Option<&LoadedServingBundle>,
    recall_policy: &RecommendationRecallPolicy,
    channels_by_id: &mut HashMap<String, Vec<RecallChannelHit>>,
    channel_counts: &mut HashMap<String, usize>,
) {
    let Some(bundle) = serving_bundle else {
        return;
    };
    let channels = channels_for_kind(recall_policy, "serving_graph");
    if channels.is_empty() {
        return;
    }
    let current_society = society_node_id(&current.society_id);
    let current_neighbors = bundle
        .edges
        .iter()
        .filter(|edge| edge.from_entity_id == current_society)
        .filter_map(|edge| {
            channels
                .iter()
                .find(|channel| graph_recall_edge(channel, &edge.edge_type))
                .map(|channel| {
                    (
                        channel.id.as_str(),
                        edge.edge_type.as_str(),
                        edge.to_entity_id.as_str(),
                    )
                })
        })
        .collect::<BTreeSet<_>>();
    if current_neighbors.is_empty() {
        return;
    }

    let society_to_property = properties
        .iter()
        .filter(|property| property.id != current.id && property.is_listable())
        .map(|property| (society_node_id(&property.society_id), property.id.as_str()))
        .collect::<HashMap<_, _>>();

    for edge in &bundle.edges {
        if edge.from_entity_id == current_society {
            continue;
        }
        for channel in &channels {
            if !graph_recall_edge(channel, &edge.edge_type) {
                continue;
            }
            if current_neighbors.contains(&(
                channel.id.as_str(),
                edge.edge_type.as_str(),
                edge.to_entity_id.as_str(),
            )) {
                if let Some(property_id) = society_to_property.get(&edge.from_entity_id) {
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
}

fn add_tantivy_recall(
    current: &Property,
    properties: &[Property],
    serving_bundle: Option<&LoadedServingBundle>,
    recall_policy: &RecommendationRecallPolicy,
    channels_by_id: &mut HashMap<String, Vec<RecallChannelHit>>,
    channel_counts: &mut HashMap<String, usize>,
) {
    let Some(bundle) = serving_bundle else {
        return;
    };
    let channels = channels_for_kind(recall_policy, "tantivy_lexical");
    if channels.is_empty() {
        return;
    }
    let query = recommendation_query(current);
    let search_limit = channels
        .iter()
        .filter_map(|channel| channel.limit)
        .max()
        .unwrap_or(recall_policy.candidate_limit);
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
        for property_id in property_ids_for_tantivy_hit(&hit, properties) {
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

fn property_ids_for_tantivy_hit(hit: &TantivyRecallHit, properties: &[Property]) -> Vec<String> {
    if let Some(id) = hit.entity_id.strip_prefix("property:") {
        return if properties.iter().any(|property| property.id == id) {
            vec![id.to_string()]
        } else {
            Vec::new()
        };
    }
    if hit.entity_id.starts_with("society:") {
        return properties
            .iter()
            .filter(|property| society_node_id(&property.society_id) == hit.entity_id)
            .map(|property| property.id.clone())
            .collect();
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
) -> Option<RecommendationBranch> {
    let current_signal = signal_score(current_score, &branch_policy.primary_signal);
    let best = candidates
        .iter()
        .filter(|candidate| !used_ids.contains(&candidate.property.id))
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
                    left.0
                        .score
                        .total_score
                        .partial_cmp(&right.0.score.total_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
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
    branches: &mut Vec<RecommendationBranch>,
    fallback_policy: &RecommendationFallbackBranchPolicy,
    target_branch_count: usize,
) {
    let mut remaining = candidates
        .iter()
        .filter(|candidate| !used_ids.contains(&candidate.property.id))
        .collect::<Vec<_>>();
    remaining.sort_by(|left, right| {
        right
            .score
            .total_score
            .partial_cmp(&left.score.total_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for candidate in remaining.into_iter().take(fallback_policy.max_items) {
        used_ids.insert(candidate.property.id.clone());
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

fn channel_strength(channels: &[RecallChannelHit]) -> f64 {
    channels.iter().map(|channel| channel.score).sum::<f64>()
}

fn same_area(current: &Property, candidate: &Property) -> bool {
    normalize(&current.area_id) == normalize(&candidate.area_id)
        || normalize(&current.area) == normalize(&candidate.area)
}

fn alias_area_match(current_area: &str, candidate_area: &str) -> bool {
    let current = normalize(current_area);
    let candidate = normalize(candidate_area);
    if current == candidate {
        return true;
    }
    area_alias_entries().iter().any(|entry| {
        let mut names = vec![normalize(&entry.canonical)];
        names.extend(entry.aliases.iter().map(|alias| normalize(alias)));
        names.iter().any(|name| name == &current) && names.iter().any(|name| name == &candidate)
    })
}

fn same_price_band(current: &Property, candidate: &Property) -> bool {
    if current.bhk != candidate.bhk || current.price_per_sqft == 0 || candidate.price_per_sqft == 0
    {
        return false;
    }
    let ratio = candidate.price_per_sqft as f64 / current.price_per_sqft as f64;
    (0.88..=1.12).contains(&ratio)
}

fn same_builder_family(current: &Property, candidate: &Property) -> bool {
    let current_builder = normalize(&current.builder_name);
    !current_builder.is_empty() && current_builder == normalize(&candidate.builder_name)
}

fn property_review_strength(property: &PropertyCard) -> f64 {
    let Some(rating) = property.google_rating.filter(|rating| *rating > 0.0) else {
        return 0.0;
    };
    let Some(review_count) = property.google_review_count.filter(|count| *count > 0) else {
        return 0.0;
    };

    rating * 100.0 + f64::from(review_count + 1).log10() * 12.0
}

fn graph_recall_edge(channel: &RecommendationRecallChannelPolicy, edge_type: &str) -> bool {
    channel
        .edge_types
        .iter()
        .any(|configured| configured == edge_type)
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

fn channels_for_kind<'a>(
    policy: &'a RecommendationRecallPolicy,
    kind: &str,
) -> Vec<&'a RecommendationRecallChannelPolicy> {
    policy
        .channels
        .iter()
        .filter(|channel| channel.enabled && channel.kind == kind)
        .collect()
}

fn ordered_branch_policies(
    policies: &[RecommendationBranchPolicy],
) -> Vec<&RecommendationBranchPolicy> {
    let mut ordered = policies.iter().enumerate().collect::<Vec<_>>();
    ordered.sort_by_key(|(index, policy)| (policy.priority, *index));
    ordered
        .into_iter()
        .map(|(_, policy)| policy)
        .collect::<Vec<_>>()
}

fn sort_branches(
    branches: &mut [RecommendationBranch],
    recall_policy: &RecommendationRecallPolicy,
    branch_policies: &[RecommendationBranchPolicy],
) {
    branches.sort_by(|left, right| {
        for tie_breaker in &recall_policy.tie_breakers {
            let ordering = match tie_breaker.as_str() {
                "review_strength_desc" => property_review_strength(&right.property)
                    .partial_cmp(&property_review_strength(&left.property))
                    .unwrap_or(std::cmp::Ordering::Equal),
                "magnitude_desc" => right
                    .magnitude
                    .partial_cmp(&left.magnitude)
                    .unwrap_or(std::cmp::Ordering::Equal),
                "branch_priority_asc" => {
                    branch_priority(&left.branch_id, recall_policy, branch_policies).cmp(
                        &branch_priority(&right.branch_id, recall_policy, branch_policies),
                    )
                }
                _ => std::cmp::Ordering::Equal,
            };
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
            }
        }
        left.property.id.cmp(&right.property.id)
    });
}

fn branch_priority(
    branch_id: &str,
    recall_policy: &RecommendationRecallPolicy,
    branch_policies: &[RecommendationBranchPolicy],
) -> u32 {
    if branch_id == recall_policy.fallback_branch.id {
        return u32::MAX;
    }
    branch_policies
        .iter()
        .find(|branch| branch.id == branch_id)
        .map(|branch| branch.priority)
        .unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::KgEntityRefs;

    #[test]
    fn configured_channel_limits_are_enforced() {
        let channel = RecommendationRecallChannelPolicy {
            id: "same_area_bhk".to_string(),
            kind: "same_area_bhk".to_string(),
            enabled: true,
            score: 0.5,
            limit: Some(1),
            edge_types: Vec::new(),
        };
        let mut channels_by_id = HashMap::new();
        let mut channel_counts = HashMap::new();

        add_configured_channel(
            &mut channels_by_id,
            &mut channel_counts,
            "property-one",
            &channel,
            channel.score,
        );
        add_configured_channel(
            &mut channels_by_id,
            &mut channel_counts,
            "property-two",
            &channel,
            channel.score,
        );

        assert!(channels_by_id.contains_key("property-one"));
        assert!(!channels_by_id.contains_key("property-two"));
    }

    #[test]
    fn disabled_recall_channels_are_not_executed() {
        let mut policy = RecommendationRecallPolicy::default();
        policy.channels = vec![RecommendationRecallChannelPolicy {
            id: "same_area_bhk".to_string(),
            kind: "same_area_bhk".to_string(),
            enabled: false,
            score: 1.0,
            limit: None,
            edge_types: Vec::new(),
        }];

        assert!(channels_for_kind(&policy, "same_area_bhk").is_empty());
    }

    #[test]
    fn branch_priority_tie_breaker_comes_from_policy() {
        let mut recall_policy = RecommendationRecallPolicy::default();
        recall_policy.tie_breakers = vec!["branch_priority_asc".to_string()];
        let branch_policies = vec![branch_policy("second", 2), branch_policy("first", 1)];
        let mut branches = vec![
            recommendation_branch("second", "property-second"),
            recommendation_branch("first", "property-first"),
        ];

        sort_branches(&mut branches, &recall_policy, &branch_policies);

        assert_eq!(branches[0].branch_id, "first");
        assert_eq!(branches[1].branch_id, "second");
    }

    fn branch_policy(id: &str, priority: u32) -> RecommendationBranchPolicy {
        RecommendationBranchPolicy {
            id: id.to_string(),
            primary_signal: "proof_strength".to_string(),
            min_delta: 0.1,
            headline: id.to_string(),
            lens: "proof".to_string(),
            priority,
        }
    }

    fn recommendation_branch(branch_id: &str, property_id: &str) -> RecommendationBranch {
        RecommendationBranch {
            branch_id: branch_id.to_string(),
            lens: BranchLens::Proof,
            headline: branch_id.to_string(),
            property: property_card(property_id),
            contrast: "contrast".to_string(),
            tradeoff: None,
            evidence_delta: EvidenceDelta {
                fact_count: 0,
                gap_count: 0,
                confidence_pct: 0,
                fact_delta: 0,
                gap_delta: 0,
            },
            channels: Vec::new(),
            magnitude: 0.25,
        }
    }

    fn property_card(id: &str) -> PropertyCard {
        PropertyCard {
            id: id.to_string(),
            kg_entity_refs: KgEntityRefs {
                property_entity_id: format!("property:{id}"),
                society_entity_id: format!("society:{id}"),
                area_entity_id: "area:test".to_string(),
                builder_entity_id: None,
                source_entity_ids: Vec::new(),
            },
            title: id.to_string(),
            area: "area".to_string(),
            price: 0,
            price_per_sqft: 0,
            bhk: 3,
            sqft: 0,
            carpet_area_sqft: 0,
            super_builtup_sqft: 0,
            society_name: "society".to_string(),
            builder_name: "builder".to_string(),
            hero_image: String::new(),
            transparency_tags: Vec::new(),
            description_summary: String::new(),
            possession_status: String::new(),
            metro_distance_mins: 0,
            floor: 0,
            total_floors: 0,
            facing: String::new(),
            google_rating: None,
            google_review_count: None,
            google_reviews_url: None,
            society_land_acres: None,
            open_space_pct: None,
            units_per_acre: None,
            root_source: None,
            project_status: None,
            project_status_display: None,
            home_state_display: None,
            builder_delivery_display: None,
            data_freshness: None,
        }
    }
}
