use std::collections::HashSet;

use crate::knowledge::graph::KnowledgeGraph;
use crate::knowledge::node::NodeType;
use crate::models::{Property, PropertyCard, Seller, Society};
use crate::routes::enrichment::{enrich_property_card_with_sellers, society_node_id};
use crate::routes::properties::{
    build_source_panels, evidence_section_from_panel, PropertyEvidenceResponse,
};
use crate::serving::LoadedServingBundle;

use super::branch::{compass_magnitude, BranchLens, EvidenceDelta, RecommendationBranch};
use super::snapshot::{summarize_evidence_sections, EvidenceSnapshot};

struct Candidate {
    property: Property,
    card: PropertyCard,
    snapshot: EvidenceSnapshot,
}

pub struct RecommendationBranchInputs<'a> {
    pub current: &'a Property,
    pub current_evidence: &'a PropertyEvidenceResponse,
    pub graph: &'a KnowledgeGraph,
    pub properties: &'a [Property],
    pub societies: &'a [Society],
    pub sellers: &'a [Seller],
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
        sellers,
        serving_bundle,
        area_median_ppsf,
    } = inputs;
    let current_snapshot = summarize_evidence_sections(&current_evidence.sections);
    let candidates = recall_candidates(
        current,
        graph,
        properties,
        societies,
        sellers,
        serving_bundle,
    );
    if candidates.is_empty() {
        return Vec::new();
    }

    let mut branches = Vec::new();
    let used_ids: &mut HashSet<String> = &mut HashSet::new();

    if let Some(branch) = pick_proof_branch(current, current_snapshot, &candidates, used_ids) {
        used_ids.insert(branch.property.id.clone());
        branches.push(branch);
    }
    if let Some(branch) = pick_value_branch(
        current,
        current_snapshot,
        area_median_ppsf,
        &candidates,
        used_ids,
    ) {
        used_ids.insert(branch.property.id.clone());
        branches.push(branch);
    }
    if let Some(branch) = pick_trust_branch(current, current_snapshot, &candidates, used_ids) {
        used_ids.insert(branch.property.id.clone());
        branches.push(branch);
    }
    if let Some(branch) = pick_commute_branch(current, current_snapshot, &candidates, used_ids) {
        used_ids.insert(branch.property.id.clone());
        branches.push(branch);
    }

    branches
}

fn recall_candidates(
    current: &Property,
    graph: &KnowledgeGraph,
    properties: &[Property],
    societies: &[Society],
    sellers: &[Seller],
    serving_bundle: Option<&LoadedServingBundle>,
) -> Vec<Candidate> {
    let mut seen = HashSet::new();
    seen.insert(current.id.clone());
    let mut ordered_ids = Vec::new();

    let soc_node_id = society_node_id(&current.society_id);
    for sim in graph.similar_to(&soc_node_id, 6, Some(NodeType::Society)) {
        if sim.similarity < 0.28 {
            continue;
        }
        if let Some(prop) = properties.iter().find(|prop| {
            society_node_id(&prop.society_id) == sim.node_id
                && prop.id != current.id
                && prop.bhk == current.bhk
        }) {
            push_candidate_id(&mut ordered_ids, &mut seen, &prop.id);
        }
    }

    let mut area_props: Vec<&Property> = properties
        .iter()
        .filter(|prop| {
            prop.id != current.id
                && prop.area_id == current.area_id
                && prop.bhk == current.bhk
                && prop.price_per_sqft > 0
        })
        .collect();
    area_props.sort_by_key(|prop| prop.price_per_sqft);
    for prop in area_props.into_iter().take(8) {
        push_candidate_id(&mut ordered_ids, &mut seen, &prop.id);
    }

    ordered_ids
        .into_iter()
        .filter_map(|id| properties.iter().find(|prop| prop.id == id))
        .map(|property| {
            let card = enrich_property_card_with_sellers(property, societies, graph, sellers);
            let source_panels = build_source_panels(
                graph,
                property,
                serving_bundle.map(|bundle| &bundle.fact_index),
            );
            let sections = source_panels
                .into_iter()
                .map(|panel| evidence_section_from_panel(panel, &card.kg_entity_refs))
                .collect::<Vec<_>>();
            let snapshot = summarize_evidence_sections(&sections);
            Candidate {
                property: property.clone(),
                card,
                snapshot,
            }
        })
        .collect()
}

fn push_candidate_id(ordered: &mut Vec<String>, seen: &mut HashSet<String>, id: &str) {
    if seen.insert(id.to_string()) {
        ordered.push(id.to_string());
    }
}

fn pick_proof_branch(
    current_property: &Property,
    current: EvidenceSnapshot,
    candidates: &[Candidate],
    used_ids: &HashSet<String>,
) -> Option<RecommendationBranch> {
    let needs_proof = current.fact_count < 10 || current.gap_count >= 2;
    if !needs_proof {
        return None;
    }

    let best = candidates
        .iter()
        .filter(|candidate| !used_ids.contains(&candidate.property.id))
        .filter(|candidate| {
            candidate.snapshot.fact_count >= current.fact_count.saturating_add(3)
                || candidate.snapshot.gap_count + 2 <= current.gap_count
        })
        .max_by(|left, right| {
            left.snapshot
                .fact_count
                .saturating_sub(left.snapshot.gap_count)
                .cmp(
                    &right
                        .snapshot
                        .fact_count
                        .saturating_sub(right.snapshot.gap_count),
                )
                .then_with(|| {
                    right
                        .snapshot
                        .confidence_pct
                        .cmp(&left.snapshot.confidence_pct)
                })
        })?;

    let fact_delta = best.snapshot.fact_count as i32 - current.fact_count as i32;
    let gap_delta = best.snapshot.gap_count as i32 - current.gap_count as i32;
    let contrast = if gap_delta < 0 {
        format!(
            "{} facts vs {} here · {} fewer gaps",
            best.snapshot.fact_count,
            current.fact_count,
            gap_delta.abs()
        )
    } else {
        format!(
            "{} facts vs {} here",
            best.snapshot.fact_count, current.fact_count
        )
    };

    let magnitude =
        compass_magnitude(fact_delta.max(0) as f32 / 18.0 + (-gap_delta).max(0) as f32 / 6.0);

    Some(RecommendationBranch {
        lens: BranchLens::Proof,
        headline: BranchLens::Proof.headline().to_string(),
        property: best.card.clone(),
        contrast,
        tradeoff: tradeoff_for_price_delta(current_property, &best.property),
        evidence_delta: EvidenceDelta {
            fact_count: best.snapshot.fact_count,
            gap_count: best.snapshot.gap_count,
            confidence_pct: best.snapshot.confidence_pct,
            fact_delta,
            gap_delta,
        },
        magnitude,
    })
}

fn pick_value_branch(
    current: &Property,
    current_snapshot: EvidenceSnapshot,
    area_median_ppsf: Option<u64>,
    candidates: &[Candidate],
    used_ids: &HashSet<String>,
) -> Option<RecommendationBranch> {
    let benchmark = area_median_ppsf.unwrap_or(current.price_per_sqft);
    if current.price_per_sqft <= benchmark {
        return None;
    }

    let best = candidates
        .iter()
        .filter(|candidate| !used_ids.contains(&candidate.property.id))
        .filter(|candidate| {
            candidate.property.price_per_sqft > 0
                && candidate.property.price_per_sqft + 50 < current.price_per_sqft
        })
        .min_by_key(|candidate| candidate.property.price_per_sqft)?;

    let savings_pct = ((current
        .price_per_sqft
        .saturating_sub(best.property.price_per_sqft)) as f64
        / current.price_per_sqft.max(1) as f64
        * 100.0)
        .round() as u64;
    if savings_pct < 4 {
        return None;
    }

    Some(RecommendationBranch {
        lens: BranchLens::Value,
        headline: BranchLens::Value.headline().to_string(),
        property: best.card.clone(),
        contrast: format!("{savings_pct}% lower per sqft in {}", best.property.area),
        tradeoff: tradeoff_for_metro_delta(current, &best.property),
        evidence_delta: delta_from_snapshots(current_snapshot, best.snapshot),
        magnitude: compass_magnitude(savings_pct as f32 / 18.0),
    })
}

fn pick_trust_branch(
    current: &Property,
    current_snapshot: EvidenceSnapshot,
    candidates: &[Candidate],
    used_ids: &HashSet<String>,
) -> Option<RecommendationBranch> {
    let current_risk = current.litigation_risk.unwrap_or(1.0)
        + (1.0 - current.document_completeness_score.unwrap_or(0.0)) * 0.35;
    if current_risk < 0.35 {
        return None;
    }

    let best = candidates
        .iter()
        .filter(|candidate| !used_ids.contains(&candidate.property.id))
        .filter(|candidate| {
            let risk = candidate.property.litigation_risk.unwrap_or(1.0)
                + (1.0 - candidate.property.document_completeness_score.unwrap_or(0.0)) * 0.35;
            risk + 0.08 < current_risk
        })
        .min_by(|left, right| {
            let left_risk = left.property.litigation_risk.unwrap_or(1.0)
                + (1.0 - left.property.document_completeness_score.unwrap_or(0.0)) * 0.35;
            let right_risk = right.property.litigation_risk.unwrap_or(1.0)
                + (1.0 - right.property.document_completeness_score.unwrap_or(0.0)) * 0.35;
            left_risk
                .partial_cmp(&right_risk)
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;

    let best_risk = best.property.litigation_risk.unwrap_or(1.0)
        + (1.0 - best.property.document_completeness_score.unwrap_or(0.0)) * 0.35;
    let magnitude = compass_magnitude(((current_risk - best_risk) / 0.4) as f32);

    Some(RecommendationBranch {
        lens: BranchLens::Trust,
        headline: BranchLens::Trust.headline().to_string(),
        property: best.card.clone(),
        contrast: format!(
            "Stronger file · {:.0}% document completeness vs {:.0}% here",
            best.property.document_completeness_score.unwrap_or(0.0) * 100.0,
            current.document_completeness_score.unwrap_or(0.0) * 100.0
        ),
        tradeoff: tradeoff_for_price_delta(current, &best.property),
        evidence_delta: delta_from_snapshots(current_snapshot, best.snapshot),
        magnitude,
    })
}

fn pick_commute_branch(
    current: &Property,
    current_snapshot: EvidenceSnapshot,
    candidates: &[Candidate],
    used_ids: &HashSet<String>,
) -> Option<RecommendationBranch> {
    if current.metro_distance_mins < 12 {
        return None;
    }

    let best = candidates
        .iter()
        .filter(|candidate| !used_ids.contains(&candidate.property.id))
        .filter(|candidate| {
            candidate.property.metro_distance_mins > 0
                && candidate.property.metro_distance_mins + 2 < current.metro_distance_mins
        })
        .min_by_key(|candidate| candidate.property.metro_distance_mins)?;

    let delta = current
        .metro_distance_mins
        .saturating_sub(best.property.metro_distance_mins);

    Some(RecommendationBranch {
        lens: BranchLens::Commute,
        headline: BranchLens::Commute.headline().to_string(),
        property: best.card.clone(),
        contrast: format!("{delta} min closer to metro"),
        tradeoff: tradeoff_for_price_delta(current, &best.property),
        evidence_delta: delta_from_snapshots(current_snapshot, best.snapshot),
        magnitude: compass_magnitude(delta as f32 / 12.0),
    })
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

fn tradeoff_for_price_delta(current: &Property, candidate: &Property) -> Option<String> {
    if candidate.price_per_sqft == 0 || current.price_per_sqft == 0 {
        return None;
    }
    if candidate.price_per_sqft > current.price_per_sqft {
        let pct = ((candidate.price_per_sqft - current.price_per_sqft) as f64
            / current.price_per_sqft as f64
            * 100.0)
            .round() as u64;
        if pct >= 4 {
            return Some(format!("Tradeoff: ~{pct}% higher per sqft"));
        }
    }
    None
}

fn tradeoff_for_metro_delta(current: &Property, candidate: &Property) -> Option<String> {
    if candidate.metro_distance_mins > current.metro_distance_mins.saturating_add(3) {
        let delta = candidate.metro_distance_mins - current.metro_distance_mins;
        return Some(format!("Tradeoff: {delta} min further from metro"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::property::KgEntityRefs;
    use crate::routes::properties::EvidenceSection;

    fn empty_evidence(property_id: &str) -> PropertyEvidenceResponse {
        PropertyEvidenceResponse {
            property_id: property_id.to_string(),
            entity_refs: KgEntityRefs {
                property_entity_id: format!("property:{property_id}"),
                society_entity_id: "society:sample".to_string(),
                area_entity_id: "area:whitefield".to_string(),
                builder_entity_id: None,
                source_entity_ids: vec![],
            },
            serving_bundle_version: None,
            sections: Vec::new(),
        }
    }

    fn property(id: &str, ppsf: u64, metro: u32, risk: f64, docs: f64) -> Property {
        Property {
            id: id.to_string(),
            title: format!("{id} title"),
            area: "Whitefield".to_string(),
            area_id: "area-whitefield".to_string(),
            city: "Bangalore".to_string(),
            society_id: format!("soc-{id}"),
            builder_name: "Builder".to_string(),
            property_type: "apartment".to_string(),
            listing_type: "sale".to_string(),
            bhk: 3,
            price: ppsf * 1_400,
            price_per_sqft: ppsf,
            carpet_area_sqft: 1_400,
            super_builtup_sqft: 1_700,
            floor: 8,
            total_floors: 18,
            facing: "East".to_string(),
            possession_status: "ready".to_string(),
            metro_distance_mins: metro,
            maintenance_cost_monthly: 4_500,
            society_quality_score: Some(0.7),
            builder_quality_score: Some(0.7),
            document_completeness_score: Some(docs),
            litigation_risk: Some(risk),
            noise_score: Some(0.3),
            sunlight_score: Some(0.7),
            airport_noise_score: Some(0.2),
            waterlogging_risk_score: Some(0.2),
            traffic_score: Some(0.4),
            days_on_market: 20,
            greenery_score: None,
            open_space_score: None,
            resale_strength_score: None,
            interest_level: None,
            saves_last_7d: None,
            offers_last_7d: None,
            images: Vec::new(),
            hero_image: String::new(),
            description_summary: String::new(),
            transparency_tags: Vec::new(),
            source_reference: "test".to_string(),
            seller_id: None,
        }
    }

    fn candidate(
        id: &str,
        ppsf: u64,
        metro: u32,
        risk: f64,
        docs: f64,
        facts: usize,
        gaps: usize,
    ) -> Candidate {
        let property = property(id, ppsf, metro, risk, docs);
        let sections = if facts == 0 && gaps == 0 {
            Vec::new()
        } else {
            vec![EvidenceSection {
                kind: "rera".to_string(),
                title: "RERA".to_string(),
                summary: String::new(),
                subtitle: String::new(),
                scope: "society".to_string(),
                relationship: None,
                priority: 10,
                constellation: "trust".to_string(),
                header_meta: "1 facts · Google".to_string(),
                confidence_pct: 80,
                source_types: vec!["Google".to_string()],
                entity_ids: vec!["society:sample".to_string()],
                presentation: crate::routes::properties::EvidencePresentation {
                    variant: "fact_list".to_string(),
                    density: "standard".to_string(),
                    max_preview_items: 4,
                },
                items: (0..facts)
                    .map(|idx| crate::routes::properties::SourceItem {
                        entity_id: "society:sample".to_string(),
                        key: format!("fact_{idx}"),
                        label: format!("Fact {idx}"),
                        value: "ok".to_string(),
                        scope: "society".to_string(),
                        relationship: None,
                        values: Vec::new(),
                        source_url: None,
                        attributions: Vec::new(),
                        source_type: "Google".to_string(),
                        confidence_pct: 80,
                        learned_at: String::new(),
                    })
                    .collect(),
                missing: (0..gaps).map(|idx| format!("gap {idx}")).collect(),
                media: Vec::new(),
                community_pulse: None,
            }]
        };
        let snapshot = summarize_evidence_sections(&sections);
        Candidate {
            card: crate::routes::enrichment::enrich_property_card(
                &property,
                &[],
                &crate::knowledge::graph::KnowledgeGraph::default(),
            ),
            property,
            snapshot,
        }
    }

    #[test]
    fn value_branch_prefers_cheaper_same_area_candidate() {
        let current = property("current", 10_000, 14, 0.2, 0.8);
        let current_evidence = empty_evidence("current");
        let candidates = vec![
            candidate("rich", 9_800, 12, 0.2, 0.8, 14, 0),
            candidate("cheap", 8_500, 16, 0.2, 0.8, 6, 1),
        ];
        let branch = pick_value_branch(
            &current,
            summarize_evidence_sections(&current_evidence.sections),
            Some(9_000),
            &candidates,
            &HashSet::new(),
        )
        .expect("value branch");
        assert_eq!(branch.property.id, "cheap");
        assert!(branch.contrast.contains("lower per sqft"));
    }
}
