use std::collections::{HashMap, HashSet};

use crate::models::Property;
use crate::routes::enrichment::society_node_id;
use crate::serving::{unique_society_aliases, ServingEntityRecord, TantivyRecallHit};

use super::analyzer;
use super::intent::SearchIntent;
use crate::dag_config::area_alias_entries;

/// In-memory recall index for local search.
///
/// This is deliberately simple: it narrows candidates with deterministic local
/// fields before ranking. It never calls external services and can be rebuilt
/// from the app-owned property list.
#[derive(Debug, Clone, Default)]
pub struct SearchIndex {
    all_ids: Vec<String>,
    by_area: HashMap<String, Vec<String>>,
    by_bhk: HashMap<u32, Vec<String>>,
    by_property_node: HashMap<String, String>,
    by_society_node: HashMap<String, Vec<String>>,
    by_named_entity_phrase: HashMap<String, Vec<String>>,
    by_token: HashMap<String, Vec<String>>,
    position_by_id: HashMap<String, usize>,
    price_by_id: HashMap<String, u64>,
}

impl SearchIndex {
    pub fn build(properties: &[Property]) -> Self {
        let mut index = Self::default();
        for property in properties {
            index.insert(property);
        }
        index
    }

    /// Build property recall mappings with canonical society identities from
    /// the promoted serving bundle. Runtime properties retain readable society
    /// slugs, while serving documents use canonical entity IDs.
    pub fn build_with_serving_entities(
        properties: &[Property],
        entities: &[ServingEntityRecord],
    ) -> Self {
        let mut index = Self::build(properties);
        for (alias, canonical_id) in unique_society_aliases(entities) {
            let Some(property_ids) = index.by_society_node.get(&alias).cloned() else {
                continue;
            };
            let canonical_property_ids = index.by_society_node.entry(canonical_id).or_default();
            for property_id in property_ids {
                push_unique(canonical_property_ids, &property_id);
            }
        }
        index
    }

    pub fn insert(&mut self, property: &Property) {
        self.position_by_id
            .entry(property.id.clone())
            .or_insert_with(|| self.all_ids.len());
        push_unique(&mut self.all_ids, &property.id);
        push_unique(
            self.by_area.entry(normalize(&property.area)).or_default(),
            &property.id,
        );
        push_unique(self.by_bhk.entry(property.bhk).or_default(), &property.id);
        self.by_property_node
            .insert(format!("property:{}", property.id), property.id.clone());
        push_unique(
            self.by_society_node
                .entry(society_node_id(&property.society_id))
                .or_default(),
            &property.id,
        );
        for phrase in named_entity_phrases(property) {
            push_unique(
                self.by_named_entity_phrase.entry(phrase).or_default(),
                &property.id,
            );
        }
        self.price_by_id.insert(property.id.clone(), property.price);

        let text = format!(
            "{} {} {} {} {} {} {} {}",
            property.title,
            property.area,
            property.city,
            property.society_id.replace('-', " "),
            property.society_id,
            property.builder_name,
            property.description_summary,
            property.transparency_tags.join(" ")
        );
        for token in analyzer::search_tokens(&text, super::schema::query_stopwords()) {
            push_unique(self.by_token.entry(token).or_default(), &property.id);
        }
    }

    pub fn recall_ids(&self, query: &str, intent: &SearchIntent) -> Vec<String> {
        let mut candidate: Option<HashSet<String>> = None;

        let named_entity_ids = self.named_entity_candidates(query);
        if !named_entity_ids.is_empty() {
            intersect_candidate(&mut candidate, named_entity_ids);
        }

        if let Some(area) = intent.area.as_deref() {
            let area_ids = self.area_candidates(area);
            if !area_ids.is_empty() {
                intersect_candidate(&mut candidate, area_ids);
            }
        }

        if let Some(bhk) = intent.bhk {
            if let Some(ids) = self.by_bhk.get(&bhk) {
                intersect_candidate(&mut candidate, ids.iter().cloned().collect());
            }
        }

        if let Some(budget_max) = intent.budget_max {
            let ids = self
                .price_by_id
                .iter()
                .filter_map(|(id, price)| {
                    if price_satisfies_budget(*price, budget_max) {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .collect();
            intersect_candidate(&mut candidate, ids);
        }

        if candidate.is_none() {
            if !intent.hard_constraints.is_empty() {
                return self.all_ids.clone();
            }
            let token_ids = self.token_candidates_ranked(query);
            if !token_ids.is_empty() {
                return token_ids;
            }
        }

        let candidate = candidate.unwrap_or_else(|| self.all_ids.iter().cloned().collect());
        self.all_ids
            .iter()
            .filter(|id| candidate.contains(*id))
            .cloned()
            .collect()
    }

    pub fn property_ids_for_entity_hits(&self, hits: &[TantivyRecallHit]) -> Vec<String> {
        let mut ids = Vec::new();
        for hit in hits {
            for property_id in self.property_ids_for_entity_id(&hit.entity_id) {
                push_unique(&mut ids, &property_id);
            }
        }
        ids
    }

    pub fn property_ids_for_entity_id(&self, entity_id: &str) -> Vec<String> {
        if let Some(property_id) = self.by_property_node.get(entity_id) {
            return vec![property_id.clone()];
        }
        if entity_id.starts_with("society:") {
            return self
                .by_society_node
                .get(entity_id)
                .cloned()
                .unwrap_or_default();
        }
        Vec::new()
    }

    pub fn property_indexes_for_ids(&self, ids: &[String]) -> Vec<usize> {
        let mut indexes = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(index) = self.position_by_id.get(id) {
                indexes.push(*index);
            }
        }
        indexes
    }

    fn area_candidates(&self, area: &str) -> HashSet<String> {
        let mut ids = HashSet::new();
        let area = normalize(area);
        self.extend_area_candidates(&area, &mut ids);

        for entry in area_alias_entries() {
            if !entry.canonical.eq_ignore_ascii_case(&area) {
                continue;
            }
            for alias in &entry.aliases {
                self.extend_area_candidates(&normalize(alias), &mut ids);
            }
        }

        ids
    }

    fn extend_area_candidates(&self, area: &str, ids: &mut HashSet<String>) {
        if let Some(exact) = self.by_area.get(area) {
            ids.extend(exact.iter().cloned());
        }

        for (indexed_area, indexed_ids) in &self.by_area {
            if indexed_area.len() >= 4
                && area.len() >= 4
                && (indexed_area.contains(area) || area.contains(indexed_area))
            {
                ids.extend(indexed_ids.iter().cloned());
            }
        }
    }

    fn token_candidates_ranked(&self, query: &str) -> Vec<String> {
        let mut scores = HashMap::<String, u32>::new();
        let mut ids = HashSet::new();
        for token in analyzer::search_tokens(query, super::schema::query_stopwords()) {
            if let Some(token_ids) = self.by_token.get(&token) {
                for id in token_ids {
                    ids.insert(id.clone());
                    *scores.entry(id.clone()).or_insert(0) += 3;
                }
                continue;
            }
            for (indexed_token, token_ids) in &self.by_token {
                if token_matches_query(&token, indexed_token) {
                    for id in token_ids {
                        ids.insert(id.clone());
                        *scores.entry(id.clone()).or_insert(0) += 1;
                    }
                }
            }
        }
        let mut ids = ids.into_iter().collect::<Vec<_>>();
        ids.sort_by(|left, right| {
            scores
                .get(right)
                .unwrap_or(&0)
                .cmp(scores.get(left).unwrap_or(&0))
                .then_with(|| self.corpus_position(left).cmp(&self.corpus_position(right)))
        });
        ids
    }

    fn named_entity_candidates(&self, query: &str) -> HashSet<String> {
        let query = normalize(query);
        if query.is_empty() {
            return HashSet::new();
        }
        self.by_named_entity_phrase
            .iter()
            .filter(|(phrase, _)| {
                phrase_has_multiple_tokens(phrase)
                    && (query_contains_phrase(&query, phrase)
                        || (phrase.len() >= query.len()
                            && phrase_has_multiple_tokens(&query)
                            && query_contains_phrase(phrase, &query)))
            })
            .flat_map(|(_, ids)| ids.iter().cloned())
            .collect()
    }

    fn corpus_position(&self, id: &str) -> usize {
        self.position_by_id.get(id).copied().unwrap_or(usize::MAX)
    }
}

/// Match a query token against indexed text, tolerating minor typos in society names.
pub fn token_matches_query(query_token: &str, candidate_token: &str) -> bool {
    if query_token == candidate_token {
        return true;
    }
    if query_token.len() < 4 || candidate_token.len() < 4 {
        return false;
    }
    if candidate_token.starts_with(query_token) || query_token.starts_with(candidate_token) {
        return true;
    }
    let max_distance = if query_token.len().max(candidate_token.len()) >= 8 {
        2
    } else {
        1
    };
    levenshtein_distance(query_token, candidate_token) <= max_distance
}

/// Match a query token against a lowercased field value.
pub fn text_field_matches_term(field_lower: &str, term: &str) -> bool {
    if field_lower.contains(term) {
        return true;
    }

    let surface_terms = analyzer::surface_tokens(term, &[]);
    if surface_terms.iter().any(|term| term.len() >= 4)
        && analyzer::surface_tokens(field_lower, &[])
            .iter()
            .any(|word| {
                surface_terms
                    .iter()
                    .any(|term| token_matches_query(term, word))
            })
    {
        return true;
    }

    let terms = analyzer::stemmed_tokens(term);
    if terms.iter().all(|term| term.len() < 4) {
        return false;
    }
    for word in analyzer::stemmed_tokens(field_lower) {
        if terms.iter().any(|term| token_matches_query(term, &word)) {
            return true;
        }
    }
    false
}

fn levenshtein_distance(left: &str, right: &str) -> usize {
    if left == right {
        return 0;
    }
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }

    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();
    let mut prev: Vec<usize> = (0..=right_chars.len()).collect();
    let mut curr = vec![0; right_chars.len() + 1];

    for (i, left_char) in left_chars.iter().enumerate() {
        curr[0] = i + 1;
        for (j, right_char) in right_chars.iter().enumerate() {
            let cost = usize::from(left_char != right_char);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[right_chars.len()]
}

fn intersect_candidate(candidate: &mut Option<HashSet<String>>, next: HashSet<String>) {
    if next.is_empty() {
        return;
    }

    match candidate {
        Some(existing) => {
            existing.retain(|id| next.contains(id));
        }
        None => {
            *candidate = Some(next);
        }
    }
}

fn push_unique(ids: &mut Vec<String>, id: &str) {
    if !ids.iter().any(|existing| existing == id) {
        ids.push(id.to_string());
    }
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

fn named_entity_phrases(property: &Property) -> Vec<String> {
    let mut phrases = Vec::new();
    push_normalized_phrase(&mut phrases, &property.title);
    push_normalized_phrase(&mut phrases, &property.society_id.replace('-', " "));
    push_normalized_phrase(&mut phrases, &property.society_id);
    phrases
}

fn push_normalized_phrase(phrases: &mut Vec<String>, value: &str) {
    let normalized = normalize(value);
    if phrase_has_multiple_tokens(&normalized) && !phrases.contains(&normalized) {
        phrases.push(normalized);
    }
}

fn phrase_has_multiple_tokens(value: &str) -> bool {
    value.split_whitespace().take(2).count() >= 2
}

fn query_contains_phrase(query: &str, phrase: &str) -> bool {
    if query == phrase {
        return true;
    }
    query
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(phrase.split_whitespace().count())
        .any(|window| window.join(" ") == phrase)
}

pub(crate) fn price_satisfies_budget(price: u64, budget_max: u64) -> bool {
    price > 0 && price <= budget_max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_matches_query_handles_waterford_typo() {
        assert!(token_matches_query("wateford", "waterford"));
        assert!(token_matches_query("waterford", "waterford"));
        assert!(!token_matches_query("xyz", "waterford"));
    }

    #[test]
    fn text_field_matches_term_handles_society_name() {
        assert!(text_field_matches_term("prestige waterford", "wateford"));
        assert!(text_field_matches_term("brigade 7 gardens", "brgade"));
    }

    #[test]
    fn recall_ids_indexes_readable_society_id_for_named_project_queries() {
        let property = test_property("prop-1", "godrej-splendour");
        let index = SearchIndex::build(&[property]);
        let intent = SearchIntent {
            area: None,
            excluded_areas: Vec::new(),
            bhk: None,
            budget_max: None,
            hard_constraints: Vec::new(),
            preferences: Vec::new(),
            positive_preferences: Vec::new(),
            negative_preferences: Vec::new(),
            accepted_tradeoffs: Vec::new(),
            unsupported_inventory_types: Vec::new(),
            buyer_archetype: None,
        };

        assert_eq!(
            index.recall_ids("Godrej Splendour", &intent),
            vec!["prop-1"]
        );
        assert_eq!(index.recall_ids("gorej", &intent), vec!["prop-1"]);
    }

    #[test]
    fn recall_ids_handles_single_deletion_in_builder_token() {
        let property = test_property("prop-1", "brigade-7-gardens");
        let index = SearchIndex::build(&[property]);
        let intent = SearchIntent {
            area: None,
            excluded_areas: Vec::new(),
            bhk: None,
            budget_max: None,
            hard_constraints: Vec::new(),
            preferences: Vec::new(),
            positive_preferences: Vec::new(),
            negative_preferences: Vec::new(),
            accepted_tradeoffs: Vec::new(),
            unsupported_inventory_types: Vec::new(),
            buyer_archetype: None,
        };

        assert_eq!(index.recall_ids("brgade", &intent), vec!["prop-1"]);
    }

    #[test]
    fn exact_society_name_recall_seeds_candidates_before_area_vocab() {
        let mut target = test_property("falcon-3bhk", "prestige-falcon-city");
        target.title = "Prestige Falcon City".to_string();
        target.area = "South Bengaluru".to_string();
        let mut area_peer = test_property("area-peer", "other-south-project");
        area_peer.title = "Other South Project".to_string();
        area_peer.area = "South Bengaluru".to_string();
        let index = SearchIndex::build(&[target, area_peer]);
        let mut intent = empty_intent();
        intent.area = Some("South Bengaluru".to_string());

        assert_eq!(
            index.recall_ids("Prestige Falcon City", &intent),
            vec!["falcon-3bhk"]
        );
    }

    #[test]
    fn unitless_hard_constraints_recall_the_local_corpus_before_fact_filtering() {
        let mut alpha = test_property("alpha", "alpha");
        alpha.area = "Whitefield".to_string();
        alpha.bhk = 2;
        let mut beta = test_property("beta", "beta");
        beta.area = "Electronic City".to_string();
        let properties = vec![alpha, beta];
        let index = SearchIndex::build(&properties);
        let intent = crate::search::intent::parse_intent(
            "homes with Google rating at least 4.2 and at least 100 reviews",
        );

        assert_eq!(
            index.recall_ids("rating and reviews", &intent),
            ["alpha", "beta"]
        );
    }

    #[test]
    fn property_ids_for_entity_hits_maps_property_and_society_hits() {
        let properties = vec![
            test_property("prop-1", "soc-one"),
            test_property("prop-2", "soc-one"),
        ];
        let index = SearchIndex::build(&properties);
        let hits = vec![
            TantivyRecallHit {
                entity_id: "property:prop-1".to_string(),
                entity_type: "property".to_string(),
                name: "Property One".to_string(),
                score: 1.0,
                matched_fields: vec!["name".to_string()],
            },
            TantivyRecallHit {
                entity_id: "society:one".to_string(),
                entity_type: "society".to_string(),
                name: "Society One".to_string(),
                score: 1.0,
                matched_fields: vec!["name".to_string()],
            },
        ];

        let ids = index.property_ids_for_entity_hits(&hits);

        assert_eq!(ids, vec!["prop-1".to_string(), "prop-2".to_string()]);
    }

    #[test]
    fn serving_aware_index_maps_canonical_society_hits_to_runtime_properties() {
        let properties = vec![
            test_property("prop-2", "century-central"),
            test_property("prop-1", "century-central"),
        ];
        let entities = vec![ServingEntityRecord {
            entity_id: "society:rera-af36618d49c94b92".to_string(),
            entity_type: "society".to_string(),
            name: "Century Central".to_string(),
            root_source: Some("rera".to_string()),
            aliases: Vec::new(),
            searchable_text: "Century Central".to_string(),
        }];
        let index = SearchIndex::build_with_serving_entities(&properties, &entities);

        assert_eq!(
            index.property_ids_for_entity_id("society:rera-af36618d49c94b92"),
            vec!["prop-2".to_string(), "prop-1".to_string()]
        );
        assert_eq!(
            index.property_ids_for_entity_id("society:century-central"),
            vec!["prop-2".to_string(), "prop-1".to_string()]
        );
    }

    fn test_property(id: &str, society_id: &str) -> Property {
        Property {
            id: id.to_string(),
            title: id.to_string(),
            area: "Whitefield".to_string(),
            area_id: "whitefield".to_string(),
            city: "Bengaluru".to_string(),
            society_id: society_id.to_string(),
            builder_name: "Builder".to_string(),
            property_type: "Apartment".to_string(),
            listing_type: "Resale".to_string(),
            bhk: 3,
            price: 10_000_000,
            price_per_sqft: 10_000,
            carpet_area_sqft: 1_000,
            super_builtup_sqft: 1_200,
            floor: 1,
            total_floors: 10,
            facing: "East".to_string(),
            possession_status: "Ready".to_string(),
            metro_distance_mins: 10,
            maintenance_cost_monthly: 5_000,
            society_quality_score: None,
            builder_quality_score: None,
            document_completeness_score: None,
            litigation_risk: None,
            noise_score: None,
            sunlight_score: None,
            airport_noise_score: None,
            waterlogging_risk_score: None,
            traffic_score: None,
            days_on_market: 1,
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
            source_reference: "unit-test".to_string(),
        }
    }

    fn empty_intent() -> SearchIntent {
        SearchIntent {
            area: None,
            excluded_areas: Vec::new(),
            bhk: None,
            budget_max: None,
            hard_constraints: Vec::new(),
            preferences: Vec::new(),
            positive_preferences: Vec::new(),
            negative_preferences: Vec::new(),
            accepted_tradeoffs: Vec::new(),
            unsupported_inventory_types: Vec::new(),
            buyer_archetype: None,
        }
    }
}
