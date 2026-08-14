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
    RecommendationBranchPolicy, ScoredSignal,
};
use crate::serving::{LoadedServingBundle, TantivyRecallHit};

use super::branch::{
    compass_magnitude, BranchLens, EvidenceDelta, RecallChannelHit, RecommendationBranch,
};
use super::snapshot::{summarize_evidence_sections, EvidenceSnapshot};

const RECALL_LIMIT: usize = 80;
const TANTIVY_RECALL_LIMIT: usize = 30;
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
    let candidates = recall_candidates(
        current,
        graph,
        properties,
        societies,
        serving_bundle,
        area_median_ppsf,
    );
    if candidates.is_empty() {
        return Vec::new();
    }

    let mut branches = Vec::new();
    let mut used_ids = HashSet::new();
    for branch_policy in &scoring_policy().recommendation_branches {
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

    if branches.len() < 3 {
        fill_with_similar_tradeoffs(
            current,
            current_snapshot,
            &current_score,
            &candidates,
            &mut used_ids,
            &mut branches,
        );
    }

    branches.sort_by(|left, right| {
        property_review_strength(&right.property)
            .partial_cmp(&property_review_strength(&left.property))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .magnitude
                    .partial_cmp(&left.magnitude)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    branches.truncate(6);
    branches
}

fn recall_candidates(
    current: &Property,
    graph: &KnowledgeGraph,
    properties: &[Property],
    societies: &[Society],
    serving_bundle: Option<&LoadedServingBundle>,
    area_median_ppsf: Option<u64>,
) -> Vec<Candidate> {
    let mut channels_by_id = HashMap::<String, Vec<RecallChannelHit>>::new();

    for property in properties {
        if property.id == current.id
            || !property.is_eligible_for(crate::buyer_eligibility::RECOMMENDATIONS_SURFACE)
        {
            continue;
        }
        if property.bhk == current.bhk && same_area(current, property) {
            add_channel(&mut channels_by_id, &property.id, "same_area_bhk", 1.0);
        }
        if property.bhk == current.bhk && alias_area_match(&current.area, &property.area) {
            add_channel(&mut channels_by_id, &property.id, "area_alias_bhk", 0.85);
        }
        if same_price_band(current, property) {
            add_channel(&mut channels_by_id, &property.id, "price_band", 0.75);
        }
        if same_builder_family(current, property) {
            add_channel(&mut channels_by_id, &property.id, "builder_family", 0.70);
        }
    }

    add_serving_graph_recall(current, properties, serving_bundle, &mut channels_by_id);
    add_tantivy_recall(current, properties, serving_bundle, &mut channels_by_id);

    let mut ordered = channels_by_id.into_iter().collect::<Vec<_>>();
    ordered.sort_by(|(left_id, left_channels), (right_id, right_channels)| {
        channel_strength(right_channels)
            .partial_cmp(&channel_strength(left_channels))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left_id.cmp(right_id))
    });

    ordered
        .into_iter()
        .take(RECALL_LIMIT)
        .filter_map(|(id, channels)| {
            let property = properties.iter().find(|property| {
                property.id == id
                    && property.is_eligible_for(crate::buyer_eligibility::RECOMMENDATIONS_SURFACE)
            })?;
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
    channels_by_id: &mut HashMap<String, Vec<RecallChannelHit>>,
) {
    let Some(bundle) = serving_bundle else {
        return;
    };
    let current_society = society_node_id(&current.society_id);
    let current_neighbors = bundle
        .edges
        .iter()
        .filter(|edge| edge.from_entity_id == current_society)
        .filter(|edge| graph_recall_edge(&edge.edge_type))
        .map(|edge| (edge.edge_type.as_str(), edge.to_entity_id.as_str()))
        .collect::<BTreeSet<_>>();
    if current_neighbors.is_empty() {
        return;
    }

    let society_to_property = properties
        .iter()
        .filter(|property| {
            property.id != current.id
                && property.is_eligible_for(crate::buyer_eligibility::RECOMMENDATIONS_SURFACE)
        })
        .map(|property| (society_node_id(&property.society_id), property.id.as_str()))
        .collect::<HashMap<_, _>>();

    for edge in &bundle.edges {
        if edge.from_entity_id == current_society || !graph_recall_edge(&edge.edge_type) {
            continue;
        }
        if current_neighbors.contains(&(edge.edge_type.as_str(), edge.to_entity_id.as_str())) {
            if let Some(property_id) = society_to_property.get(&edge.from_entity_id) {
                add_channel(channels_by_id, property_id, "serving_graph", 0.68);
            }
        }
    }
}

fn add_tantivy_recall(
    current: &Property,
    properties: &[Property],
    serving_bundle: Option<&LoadedServingBundle>,
    channels_by_id: &mut HashMap<String, Vec<RecallChannelHit>>,
) {
    let Some(bundle) = serving_bundle else {
        return;
    };
    let query = recommendation_query(current);
    let Ok(hits) = bundle.recall_index.search(&query, TANTIVY_RECALL_LIMIT) else {
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
                add_channel(channels_by_id, &property_id, "tantivy_lexical", score);
            }
        }
    }
}

fn property_ids_for_tantivy_hit(hit: &TantivyRecallHit, properties: &[Property]) -> Vec<String> {
    if let Some(id) = hit.entity_id.strip_prefix("property:") {
        return if properties.iter().any(|property| {
            property.id == id
                && property.is_eligible_for(crate::buyer_eligibility::RECOMMENDATIONS_SURFACE)
        }) {
            vec![id.to_string()]
        } else {
            Vec::new()
        };
    }
    if hit.entity_id.starts_with("society:") {
        return properties
            .iter()
            .filter(|property| {
                society_node_id(&property.society_id) == hit.entity_id
                    && property.is_eligible_for(crate::buyer_eligibility::RECOMMENDATIONS_SURFACE)
            })
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

    for candidate in remaining.into_iter().take(3) {
        used_ids.insert(candidate.property.id.clone());
        branches.push(RecommendationBranch {
            branch_id: "similar_tradeoff".to_string(),
            lens: BranchLens::Proof,
            headline: "Similar tradeoff".to_string(),
            property: candidate.card.clone(),
            contrast: best_available_contrast(current, current_score, candidate),
            tradeoff: tradeoff_for_candidate(current, current_score, candidate),
            evidence_delta: delta_from_snapshots(current_snapshot, candidate.snapshot),
            channels: candidate.channels.clone(),
            magnitude: compass_magnitude(channel_strength(&candidate.channels) as f32 / 2.0),
        });
        if branches.len() >= 3 {
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

fn graph_recall_edge(edge_type: &str) -> bool {
    matches!(
        edge_type,
        "in_area" | "served_by_road" | "near_place" | "built_by" | "near_transit"
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tantivy_hits_do_not_recall_buyer_ineligible_properties() {
        let ready = property("ready-home", "shared-society", true);
        let blocked = property("blocked-home", "shared-society", false);
        let properties = vec![ready, blocked];

        let property_hit = TantivyRecallHit {
            entity_id: "property:blocked-home".to_string(),
            entity_type: "property".to_string(),
            name: "Blocked home".to_string(),
            score: 1.0,
            matched_fields: vec!["name".to_string()],
        };
        assert!(property_ids_for_tantivy_hit(&property_hit, &properties).is_empty());

        let society_hit = TantivyRecallHit {
            entity_id: "society:shared-society".to_string(),
            entity_type: "society".to_string(),
            name: "Shared Society".to_string(),
            score: 1.0,
            matched_fields: vec!["name".to_string()],
        };
        assert_eq!(
            property_ids_for_tantivy_hit(&society_hit, &properties),
            ["ready-home"]
        );
    }

    fn property(id: &str, society_id: &str, eligible: bool) -> Property {
        let mut buyer_eligibility = crate::buyer_eligibility::evaluate_signals(
            crate::buyer_eligibility::BuyerEligibilitySignals::complete_without_media(),
        );
        if !eligible {
            let decision = buyer_eligibility
                .surfaces
                .get_mut(crate::buyer_eligibility::RECOMMENDATIONS_SURFACE)
                .expect("recommendations decision");
            decision.eligible = false;
            decision.reason_codes = vec!["missing_price".to_string()];
        }
        Property {
            id: id.to_string(),
            title: "3 BHK in Test Home".to_string(),
            area: "Test Area".to_string(),
            area_id: "test-area".to_string(),
            city: "Test City".to_string(),
            society_id: society_id.to_string(),
            builder_name: "Test Builder".to_string(),
            property_type: "Apartment".to_string(),
            listing_type: "Resale".to_string(),
            bhk: 3,
            price: 10_000_000,
            price_per_sqft: 10_000,
            carpet_area_sqft: 1_000,
            super_builtup_sqft: 1_200,
            floor: 1,
            total_floors: 10,
            facing: String::new(),
            possession_status: String::new(),
            status: Default::default(),
            buyer_eligibility,
            metro_distance_mins: 0,
            maintenance_cost_monthly: 0,
            society_quality_score: None,
            builder_quality_score: None,
            document_completeness_score: None,
            litigation_risk: None,
            noise_score: None,
            sunlight_score: None,
            airport_noise_score: None,
            waterlogging_risk_score: None,
            traffic_score: None,
            days_on_market: 0,
            greenery_score: None,
            open_space_score: None,
            resale_strength_score: None,
            interest_level: None,
            saves_last_7d: None,
            offers_last_7d: None,
            images: Vec::new(),
            hero_image: String::new(),
            media: Vec::new(),
            description_summary: String::new(),
            transparency_tags: Vec::new(),
            source_reference: String::new(),
        }
    }
}
