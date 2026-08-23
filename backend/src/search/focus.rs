//! Search result focus rails: named-society matches first, then more homes.
//!
//! Sibling BHK configs at a named society stay out of the hard BHK filter so the
//! UI can show a quiet "+" — not a search mistake, just other configurations.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::Serialize;

use crate::dag_config::{area_alias_entries, search_resolution_config};
use crate::knowledge::KnowledgeGraph;
use crate::models::{Property, PropertyCard, Society};
use crate::routes::enrichment::{enrich_property_card, society_node_id};
use crate::search::intent::SearchIntent;
use crate::search::resolver::{is_resolvable_entity_name, query_contains_lower_text};
use crate::search::text::enrich_card_from_serving_facts;
use crate::search::SearchResultCard;
use crate::serving::ServingFactIndex;

pub const FOCUS_MODE_NAMED_SOCIETY: &str = "named_society";
pub const FOCUS_MODE_RANKED_MATCHES: &str = "ranked_matches";

/// Journey-aware rails for search results.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResultFocus {
    /// `named_society` or `ranked_matches`
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub society_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub society_name: Option<String>,
    /// Asked configuration(s) / primary matches for this query.
    pub focus_results: Vec<SearchResultCard>,
    /// Same society, other BHKs — expand behind a quiet "+" in the UI.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sibling_configs: Vec<SearchResultCard>,
    /// Alternatives / weaker matches shown under "More homes".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub more_homes: Vec<SearchResultCard>,
}

pub struct FocusBuildInputs<'a> {
    pub query: &'a str,
    pub intent: &'a SearchIntent,
    pub results: &'a [SearchResultCard],
    pub properties: &'a [Property],
    pub society_names: &'a HashMap<String, String>,
    pub societies: &'a [Society],
    pub serving_facts: Option<&'a ServingFactIndex>,
    pub graph: Option<&'a KnowledgeGraph>,
}

pub fn build_search_result_focus(inputs: FocusBuildInputs<'_>) -> Option<SearchResultFocus> {
    if inputs.results.is_empty() {
        return None;
    }

    let query_lower = inputs.query.to_lowercase();
    // Named-society focus only from ranked hits — never invent a focus society
    // from the full catalog (that demotes real ranked matches under More homes).
    if let Some((society_id, society_name)) =
        resolve_named_society_focus(&query_lower, inputs.intent.area.as_deref(), inputs.results)
    {
        return Some(build_named_society_focus(
            &society_id,
            &society_name,
            inputs.intent.bhk,
            inputs.results,
            inputs.properties,
            inputs.society_names,
            inputs.societies,
            inputs.serving_facts,
            inputs.graph,
        ));
    }

    Some(build_ranked_matches_focus(inputs.results))
}

fn resolve_named_society_focus(
    query_lower: &str,
    resolved_area: Option<&str>,
    results: &[SearchResultCard],
) -> Option<(String, String)> {
    let corpus_names: Vec<&str> = results
        .iter()
        .map(|r| r.card.society_name.as_str())
        .collect();
    let mut best: Option<(String, String, usize)> = None;

    for result in results {
        let name = result.card.society_name.trim();
        if name.is_empty() {
            continue;
        }
        if !query_focuses_society(query_lower, resolved_area, name, &corpus_names) {
            continue;
        }
        let score = society_focus_score(query_lower, resolved_area, name, &corpus_names);
        if best
            .as_ref()
            .is_none_or(|(_, _, best_score)| score > *best_score)
        {
            best = Some((
                result.card.kg_entity_refs.society_entity_id.clone(),
                name.to_string(),
                score,
            ));
        }
    }

    best.map(|(id, name, _)| (id, name))
}

fn query_focuses_society(
    query_lower: &str,
    resolved_area: Option<&str>,
    society_name: &str,
    corpus_names: &[&str],
) -> bool {
    let resolution_config = search_resolution_config();
    if !is_resolvable_entity_name(society_name, resolution_config) {
        return false;
    }
    if query_contains_lower_text(query_lower, society_name) {
        return true;
    }
    // "3bhk in waterford" focuses Prestige Waterford via a distinctive project token.
    // Never treat resolved locality/area language as a society focus.
    distinctive_society_tokens(society_name, corpus_names).any(|token| {
        !is_area_language_token(&token, resolved_area)
            && query_contains_lower_text(query_lower, &token)
    })
}

fn society_focus_score(
    query_lower: &str,
    resolved_area: Option<&str>,
    society_name: &str,
    corpus_names: &[&str],
) -> usize {
    if query_contains_lower_text(query_lower, society_name) {
        return society_name.len() * 10;
    }
    distinctive_society_tokens(society_name, corpus_names)
        .filter(|token| {
            !is_area_language_token(token, resolved_area)
                && query_contains_lower_text(query_lower, token)
        })
        .map(|token| token.len())
        .max()
        .unwrap_or(0)
}

fn distinctive_society_tokens<'a>(
    society_name: &'a str,
    corpus_names: &'a [&str],
) -> impl Iterator<Item = String> + 'a {
    society_name
        .split(|c: char| !c.is_alphanumeric())
        .map(str::trim)
        .filter(|token| token.len() >= 4)
        .map(str::to_lowercase)
        .filter(|token| {
            let hits = corpus_names
                .iter()
                .filter(|name| {
                    name.to_lowercase()
                        .split(|c: char| !c.is_alphanumeric())
                        .any(|part| part == token)
                })
                .count();
            // Unique or near-unique project tokens only — skip shared builder brands.
            hits > 0 && hits <= 2
        })
}

fn is_area_language_token(token: &str, resolved_area: Option<&str>) -> bool {
    area_language_tokens().contains(token)
        || resolved_area.is_some_and(|area| {
            area.split(|character: char| !character.is_alphanumeric())
                .any(|part| part.eq_ignore_ascii_case(token))
        })
}

fn area_language_tokens() -> &'static HashSet<String> {
    static TOKENS: OnceLock<HashSet<String>> = OnceLock::new();
    TOKENS.get_or_init(|| {
        let mut tokens = HashSet::new();
        for entry in area_alias_entries() {
            for raw in std::iter::once(entry.canonical.as_str())
                .chain(entry.aliases.iter().map(String::as_str))
            {
                for part in raw.split(|c: char| !c.is_alphanumeric()) {
                    let part = part.trim().to_ascii_lowercase();
                    if part.len() >= 4 {
                        tokens.insert(part);
                    }
                }
            }
        }
        tokens
    })
}

#[allow(clippy::too_many_arguments)]
fn build_named_society_focus(
    society_entity_id: &str,
    society_name: &str,
    asked_bhk: Option<u32>,
    results: &[SearchResultCard],
    properties: &[Property],
    society_names: &HashMap<String, String>,
    societies: &[Society],
    serving_facts: Option<&ServingFactIndex>,
    graph: Option<&KnowledgeGraph>,
) -> SearchResultFocus {
    let mut focus_results = Vec::new();
    let mut more_homes = Vec::new();
    let mut focus_ids = HashSet::new();

    for result in results {
        if society_ids_match(society_entity_id, &result.card) {
            focus_ids.insert(result.card.id.clone());
            focus_results.push(result.clone());
        } else {
            more_homes.push(result.clone());
        }
    }

    let raw_society_id = strip_society_prefix(society_entity_id);
    // Only attach sibling configs when the asked config actually ranked.
    let sibling_configs = if focus_results.is_empty() {
        Vec::new()
    } else {
        sibling_config_cards(
            raw_society_id,
            society_name,
            asked_bhk,
            &focus_ids,
            properties,
            society_names,
            societies,
            serving_facts,
            graph,
        )
    };

    SearchResultFocus {
        mode: FOCUS_MODE_NAMED_SOCIETY.to_string(),
        society_id: Some(society_entity_id.to_string()),
        society_name: Some(society_name.to_string()),
        focus_results,
        sibling_configs,
        more_homes,
    }
}

fn build_ranked_matches_focus(results: &[SearchResultCard]) -> SearchResultFocus {
    let mut focus_results = Vec::new();
    let mut more_homes = Vec::new();

    for result in results {
        if is_primary_ranked_match(result) {
            focus_results.push(result.clone());
        } else {
            more_homes.push(result.clone());
        }
    }

    if focus_results.is_empty() {
        focus_results = results.to_vec();
        more_homes.clear();
    }

    SearchResultFocus {
        mode: FOCUS_MODE_RANKED_MATCHES.to_string(),
        society_id: None,
        society_name: None,
        focus_results,
        sibling_configs: Vec::new(),
        more_homes,
    }
}

fn is_primary_ranked_match(result: &SearchResultCard) -> bool {
    result.match_score >= super::schema::ranking_policy().ranked_focus_min_match_score
}

fn society_ids_match(society_entity_id: &str, card: &PropertyCard) -> bool {
    let card_id = card.kg_entity_refs.society_entity_id.trim();
    if !card_id.is_empty()
        && (card_id.eq_ignore_ascii_case(society_entity_id)
            || strip_society_prefix(card_id)
                .eq_ignore_ascii_case(strip_society_prefix(society_entity_id)))
    {
        return true;
    }
    card.society_name
        .trim()
        .eq_ignore_ascii_case(strip_display_only(society_entity_id))
}

fn strip_society_prefix(society_entity_id: &str) -> &str {
    society_entity_id
        .strip_prefix("society:")
        .unwrap_or(society_entity_id)
}

fn strip_display_only(society_entity_id: &str) -> &str {
    strip_society_prefix(society_entity_id)
}

#[allow(clippy::too_many_arguments)]
fn sibling_config_cards(
    raw_society_id: &str,
    society_name: &str,
    asked_bhk: Option<u32>,
    exclude_ids: &HashSet<String>,
    properties: &[Property],
    society_names: &HashMap<String, String>,
    societies: &[Society],
    serving_facts: Option<&ServingFactIndex>,
    graph: Option<&KnowledgeGraph>,
) -> Vec<SearchResultCard> {
    let Some(asked_bhk) = asked_bhk else {
        return Vec::new();
    };

    let mut siblings: Vec<SearchResultCard> = properties
        .iter()
        .filter(|property| {
            property.is_listable()
                && !exclude_ids.contains(&property.id)
                && property.bhk != asked_bhk
                && property_belongs_to_society(
                    property,
                    raw_society_id,
                    society_names,
                    society_name,
                )
        })
        .map(|property| {
            sibling_result_card(property, society_name, societies, serving_facts, graph)
        })
        .collect();

    siblings.sort_by(|a, b| {
        a.card
            .bhk
            .cmp(&b.card.bhk)
            .then_with(|| a.card.price.cmp(&b.card.price))
            .then_with(|| a.card.id.cmp(&b.card.id))
    });
    siblings
}

fn property_belongs_to_society(
    property: &Property,
    raw_society_id: &str,
    society_names: &HashMap<String, String>,
    society_name: &str,
) -> bool {
    if property.society_id.eq_ignore_ascii_case(raw_society_id)
        || society_node_id(&property.society_id)
            .eq_ignore_ascii_case(&society_node_id(raw_society_id))
    {
        return true;
    }
    society_names
        .get(&property.society_id)
        .is_some_and(|name| name.eq_ignore_ascii_case(society_name))
}

fn sibling_result_card(
    property: &Property,
    society_name: &str,
    societies: &[Society],
    serving_facts: Option<&ServingFactIndex>,
    graph: Option<&KnowledgeGraph>,
) -> SearchResultCard {
    let mut card = if let Some(graph) = graph {
        enrich_property_card(property, societies, graph)
    } else {
        let name = if society_name.is_empty() {
            property.society_id.clone()
        } else {
            society_name.to_string()
        };
        PropertyCard {
            id: property.id.clone(),
            kg_entity_refs: crate::models::KgEntityRefs {
                property_entity_id: format!("property:{}", property.id),
                society_entity_id: society_node_id(&property.society_id),
                area_entity_id: format!("area:{}", property.area_id),
                builder_entity_id: None,
                source_entity_ids: Vec::new(),
            },
            title: property.title.clone(),
            area: property.area.clone(),
            price: property.price,
            price_min: property.price_min,
            price_max: property.price_max,
            price_per_sqft: property.price_per_sqft,
            bhk: property.bhk,
            sqft: property.carpet_area_sqft,
            carpet_area_sqft: property.carpet_area_sqft,
            super_builtup_sqft: property.super_builtup_sqft,
            society_name: name,
            builder_name: property.builder_name.clone(),
            images: property.images.clone(),
            hero_image: property.hero_image.clone(),
            transparency_tags: property.transparency_tags.iter().take(3).cloned().collect(),
            description_summary: property.description_summary.clone(),
            possession_status: property.possession_status.clone(),
            metro_distance_mins: property.metro_distance_mins,
            floor: property.floor,
            total_floors: property.total_floors,
            facing: property.facing.clone(),
            google_rating: None,
            google_review_count: None,
            google_reviews_url: None,
            society_land_acres: None,
            open_space_pct: None,
            root_source: None,
            project_status: None,
            project_status_display: None,
            home_state_display: None,
            builder_delivery_display: None,
            data_freshness: None,
            floor_plan_preview_url: None,
            plan_carpet_area_sqft: None,
            plan_sale_area_sqft: None,
            plan_configuration_type: None,
            decision_labels: Vec::new(),
            decision_check_summary: None,
        }
    };
    if let Some(serving_facts) = serving_facts {
        enrich_card_from_serving_facts(&mut card, serving_facts, &property.society_id);
    }

    SearchResultCard {
        card,
        match_score: 0.0,
        match_label: "Also available".to_string(),
        match_reason: format!(
            "Other configuration at {} · {}BHK",
            society_name, property.bhk
        ),
        match_tier: "supported".to_string(),
        tradeoff_label: Some("Other configuration".to_string()),
        match_explanation: None,
        proof_focuses: Vec::new(),
        confidence_score: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::KgEntityRefs;

    fn card(
        id: &str,
        society: &str,
        society_entity_id: &str,
        bhk: u32,
        score: f64,
        label: &str,
    ) -> SearchResultCard {
        SearchResultCard {
            card: PropertyCard {
                id: id.to_string(),
                kg_entity_refs: KgEntityRefs {
                    property_entity_id: format!("property:{id}"),
                    society_entity_id: society_entity_id.to_string(),
                    area_entity_id: "area:whitefield".to_string(),
                    builder_entity_id: None,
                    source_entity_ids: Vec::new(),
                },
                title: format!("{bhk} BHK in {society}"),
                area: "Whitefield".to_string(),
                price: 30_000_000,
                price_min: None,
                price_max: None,
                price_per_sqft: 15_000,
                bhk,
                sqft: 1800,
                carpet_area_sqft: 1800,
                super_builtup_sqft: 2000,
                society_name: society.to_string(),
                builder_name: "Builder".to_string(),
                images: Vec::new(),
                hero_image: String::new(),
                transparency_tags: Vec::new(),
                description_summary: String::new(),
                possession_status: "Ready".to_string(),
                metro_distance_mins: 10,
                floor: 5,
                total_floors: 20,
                facing: "East".to_string(),
                google_rating: None,
                google_review_count: None,
                google_reviews_url: None,
                society_land_acres: None,
                open_space_pct: None,
                root_source: None,
                project_status: None,
                project_status_display: None,
                home_state_display: None,
                builder_delivery_display: None,
                data_freshness: None,
                floor_plan_preview_url: None,
                plan_carpet_area_sqft: None,
                plan_sale_area_sqft: None,
                plan_configuration_type: None,
                decision_labels: Vec::new(),
                decision_check_summary: None,
            },
            match_score: score,
            match_label: label.to_string(),
            match_reason: "matches".to_string(),
            match_tier: "exact".to_string(),
            tradeoff_label: None,
            match_explanation: None,
            proof_focuses: Vec::new(),
            confidence_score: None,
        }
    }

    fn property(id: &str, society_id: &str, bhk: u32) -> Property {
        Property {
            id: id.to_string(),
            title: format!("{bhk} BHK"),
            area: "Whitefield".to_string(),
            area_id: "whitefield".to_string(),
            city: "Bengaluru".to_string(),
            society_id: society_id.to_string(),
            builder_name: "Prestige".to_string(),
            property_type: "apartment".to_string(),
            listing_type: "sale".to_string(),
            bhk,
            price: 20_000_000 + u64::from(bhk) * 1_000_000,
            price_min: None,
            price_max: None,
            price_per_sqft: 12_000,
            carpet_area_sqft: 1000 + bhk * 200,
            super_builtup_sqft: 1200 + bhk * 200,
            floor: 4,
            total_floors: 18,
            facing: "East".to_string(),
            possession_status: "Ready".to_string(),
            metro_distance_mins: 12,
            maintenance_cost_monthly: 5000,
            society_quality_score: None,
            builder_quality_score: None,
            document_completeness_score: None,
            litigation_risk: None,
            noise_score: None,
            sunlight_score: None,
            airport_noise_score: None,
            waterlogging_risk_score: None,
            traffic_score: None,
            days_on_market: 10,
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
            source_reference: String::new(),
        }
    }

    #[test]
    fn named_society_query_splits_focus_siblings_and_more_homes() {
        let results = vec![
            card(
                "waterford-3",
                "Prestige Waterford",
                "society:prestige-waterford",
                3,
                0.9,
                "Strong match",
            ),
            card(
                "splendour-3",
                "Godrej Splendour",
                "society:godrej-splendour",
                3,
                0.5,
                "Good match",
            ),
        ];
        let properties = vec![
            property("waterford-1", "prestige-waterford", 1),
            property("waterford-2", "prestige-waterford", 2),
            property("waterford-3", "prestige-waterford", 3),
            property("waterford-4", "prestige-waterford", 4),
            property("splendour-3", "godrej-splendour", 3),
        ];
        let mut society_names = HashMap::new();
        society_names.insert(
            "prestige-waterford".to_string(),
            "Prestige Waterford".to_string(),
        );
        society_names.insert(
            "godrej-splendour".to_string(),
            "Godrej Splendour".to_string(),
        );

        let intent = SearchIntent {
            area: None,
            excluded_areas: Vec::new(),
            excluded_societies: Vec::new(),
            excluded_builders: Vec::new(),
            areas: Vec::new(),
            bhk: Some(3),
            bhks: Vec::new(),
            exclude_bhks: Vec::new(),
            bhk_spans: Vec::new(),
            budget_min: None,
            budget_max: None,
            hard_constraints: Vec::new(),
            preferences: Vec::new(),
            positive_preferences: Vec::new(),
            negative_preferences: Vec::new(),
            ranking_priorities: Vec::new(),
            accepted_tradeoffs: Vec::new(),
            unsupported_inventory_types: Vec::new(),
            buyer_archetype: None,
        };

        let focus = build_search_result_focus(FocusBuildInputs {
            query: "3bhk in waterford",
            intent: &intent,
            results: &results,
            properties: &properties,
            society_names: &society_names,
            societies: &[],
            serving_facts: None,
            graph: None,
        })
        .expect("focus rails");

        assert_eq!(focus.mode, FOCUS_MODE_NAMED_SOCIETY);
        assert_eq!(focus.focus_results.len(), 1);
        assert_eq!(focus.focus_results[0].card.id, "waterford-3");
        assert_eq!(
            focus
                .sibling_configs
                .iter()
                .map(|c| c.card.bhk)
                .collect::<Vec<_>>(),
            vec![1, 2, 4]
        );
        assert_eq!(focus.more_homes.len(), 1);
        assert_eq!(focus.more_homes[0].card.id, "splendour-3");
    }

    #[test]
    fn ranked_soft_query_keeps_strong_matches_in_focus() {
        let results = vec![
            card("a", "Alpha", "society:alpha", 3, 0.8, "Strong match"),
            card("b", "Beta", "society:beta", 3, 0.55, "Good match"),
            card("c", "Gamma", "society:gamma", 3, 0.2, "Weak match"),
        ];
        let intent = SearchIntent {
            area: Some("Kadugodi".to_string()),
            excluded_areas: Vec::new(),
            excluded_societies: Vec::new(),
            excluded_builders: Vec::new(),
            areas: Vec::new(),
            bhk: Some(3),
            bhks: Vec::new(),
            exclude_bhks: Vec::new(),
            bhk_spans: Vec::new(),
            budget_min: None,
            budget_max: None,
            hard_constraints: Vec::new(),
            preferences: vec!["metro".to_string()],
            positive_preferences: Vec::new(),
            negative_preferences: Vec::new(),
            ranking_priorities: Vec::new(),
            accepted_tradeoffs: Vec::new(),
            unsupported_inventory_types: Vec::new(),
            buyer_archetype: None,
        };
        let society_names = HashMap::new();
        let focus = build_search_result_focus(FocusBuildInputs {
            query: "3bhk near metro near kadugodi",
            intent: &intent,
            results: &results,
            properties: &[],
            society_names: &society_names,
            societies: &[],
            serving_facts: None,
            graph: None,
        })
        .expect("focus rails");

        assert_eq!(focus.mode, FOCUS_MODE_RANKED_MATCHES);
        assert_eq!(focus.focus_results.len(), 2);
        assert!(focus.sibling_configs.is_empty());
        assert_eq!(focus.more_homes.len(), 1);
        assert_eq!(focus.more_homes[0].card.id, "c");
    }

    #[test]
    fn area_query_does_not_become_named_society_via_locality_token() {
        // Even with a single "* Whitefield" society in ranked results, locality
        // language must stay ranked_matches — otherwise display ranking demotes peers.
        let results = vec![
            card(
                "green-3",
                "Green Acre Whitefield",
                "society:green-acre-whitefield",
                3,
                0.8,
                "Strong match",
            ),
            card(
                "other-3",
                "Other Homes",
                "society:other-homes",
                3,
                0.7,
                "Good match",
            ),
        ];
        let intent = SearchIntent {
            area: Some("Whitefield".to_string()),
            excluded_areas: Vec::new(),
            excluded_societies: Vec::new(),
            excluded_builders: Vec::new(),
            areas: Vec::new(),
            bhk: Some(3),
            bhks: Vec::new(),
            exclude_bhks: Vec::new(),
            bhk_spans: Vec::new(),
            budget_min: None,
            budget_max: None,
            hard_constraints: Vec::new(),
            preferences: Vec::new(),
            positive_preferences: Vec::new(),
            negative_preferences: Vec::new(),
            ranking_priorities: Vec::new(),
            accepted_tradeoffs: Vec::new(),
            unsupported_inventory_types: Vec::new(),
            buyer_archetype: None,
        };
        let society_names = HashMap::new();
        let focus = build_search_result_focus(FocusBuildInputs {
            query: "3bhk whitefield",
            intent: &intent,
            results: &results,
            properties: &[],
            society_names: &society_names,
            societies: &[],
            serving_facts: None,
            graph: None,
        })
        .expect("focus rails");

        assert_eq!(focus.mode, FOCUS_MODE_RANKED_MATCHES);
        assert_eq!(focus.focus_results.len(), 2);
        assert!(focus.more_homes.is_empty());
        assert!(focus.sibling_configs.is_empty());
    }

    #[test]
    fn focus_rails_preserve_ranked_result_membership() {
        let results = vec![
            card(
                "waterford-3",
                "Prestige Waterford",
                "society:prestige-waterford",
                3,
                0.9,
                "Strong match",
            ),
            card(
                "splendour-3",
                "Godrej Splendour",
                "society:godrej-splendour",
                3,
                0.5,
                "Good match",
            ),
        ];
        let intent = SearchIntent {
            area: None,
            excluded_areas: Vec::new(),
            excluded_societies: Vec::new(),
            excluded_builders: Vec::new(),
            areas: Vec::new(),
            bhk: Some(3),
            bhks: Vec::new(),
            exclude_bhks: Vec::new(),
            bhk_spans: Vec::new(),
            budget_min: None,
            budget_max: None,
            hard_constraints: Vec::new(),
            preferences: Vec::new(),
            positive_preferences: Vec::new(),
            negative_preferences: Vec::new(),
            ranking_priorities: Vec::new(),
            accepted_tradeoffs: Vec::new(),
            unsupported_inventory_types: Vec::new(),
            buyer_archetype: None,
        };
        let society_names = HashMap::new();
        let focus = build_search_result_focus(FocusBuildInputs {
            query: "3bhk in waterford",
            intent: &intent,
            results: &results,
            properties: &[property("waterford-1", "prestige-waterford", 1)],
            society_names: &society_names,
            societies: &[],
            serving_facts: None,
            graph: None,
        })
        .expect("focus rails");

        let mut rebuilt: Vec<_> = focus
            .focus_results
            .iter()
            .chain(focus.more_homes.iter())
            .map(|r| r.card.id.as_str())
            .collect();
        rebuilt.sort();
        let mut flat: Vec<_> = results.iter().map(|r| r.card.id.as_str()).collect();
        flat.sort();
        assert_eq!(rebuilt, flat);
        assert!(focus
            .sibling_configs
            .iter()
            .all(|sib| !flat.contains(&sib.card.id.as_str())));
    }
}
