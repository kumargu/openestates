use std::collections::{HashMap, HashSet};

use crate::models::Property;
use crate::routes::enrichment::society_node_id;
use crate::serving::TantivyRecallHit;

use super::analyzer;
use super::intent::SearchIntent;
use super::semantic::SemanticRecallHit;
use crate::dag_config::area_alias_entries;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedAreaResolution {
    pub name: String,
    pub indexed_area: String,
    pub candidate_count: usize,
}

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
            if let Some(property_id) = self.by_property_node.get(&hit.entity_id) {
                push_unique(&mut ids, property_id);
            } else if hit.entity_id.starts_with("society:") {
                if let Some(property_ids) = self.by_society_node.get(&hit.entity_id) {
                    for property_id in property_ids {
                        push_unique(&mut ids, property_id);
                    }
                }
            }
        }
        ids
    }

    pub fn property_scores_for_semantic_hits(
        &self,
        hits: &[SemanticRecallHit],
    ) -> HashMap<String, f64> {
        let mut scores = HashMap::new();
        for hit in hits {
            if let Some(property_id) = self.by_property_node.get(&hit.entity_id) {
                merge_score(&mut scores, property_id, hit.score);
                continue;
            }
            if hit.entity_id.starts_with("society:") {
                if let Some(property_ids) = self.by_society_node.get(&hit.entity_id) {
                    for property_id in property_ids {
                        merge_score(&mut scores, property_id, hit.score);
                    }
                }
            }
        }
        scores
    }

    pub fn property_ids_for_semantic_hits(&self, hits: &[SemanticRecallHit]) -> Vec<String> {
        let mut ids = Vec::new();
        for hit in hits {
            if let Some(property_id) = self.by_property_node.get(&hit.entity_id) {
                push_unique(&mut ids, property_id);
                continue;
            }
            if hit.entity_id.starts_with("society:") {
                if let Some(property_ids) = self.by_society_node.get(&hit.entity_id) {
                    for property_id in property_ids {
                        push_unique(&mut ids, property_id);
                    }
                }
            }
        }
        ids
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

    pub fn resolve_query_area(
        &self,
        query: &str,
        excluded_areas: &[String],
    ) -> Option<IndexedAreaResolution> {
        let excluded = excluded_areas
            .iter()
            .map(|area| normalize(area))
            .collect::<HashSet<_>>();
        self.resolve_query_area_matches(query)
            .into_iter()
            .filter(|resolution| !excluded.contains(&normalize(&resolution.name)))
            .filter(|resolution| !area_term_is_excluded(query, &resolution.name))
            .max_by(|left, right| compare_area_resolution(left, right))
    }

    pub fn resolve_excluded_query_areas(&self, query: &str) -> Vec<IndexedAreaResolution> {
        let mut areas = Vec::new();
        for (indexed_area, ids) in &self.by_area {
            if indexed_area.trim().is_empty() {
                continue;
            }
            let mut terms = area_match_terms(indexed_area);
            push_area_term(&mut terms, &normalize_area_text_for_exclusion(indexed_area));
            for term in terms {
                if area_term_is_excluded(query, &term) {
                    areas.push(IndexedAreaResolution {
                        name: display_area_term(&term),
                        indexed_area: indexed_area.clone(),
                        candidate_count: ids.len(),
                    });
                    break;
                }
            }
        }
        areas.sort_by(compare_area_resolution);
        areas.dedup_by(|left, right| left.name.eq_ignore_ascii_case(&right.name));
        areas
    }

    fn resolve_query_area_matches(&self, query: &str) -> Vec<IndexedAreaResolution> {
        let query_tokens = analyzer::search_tokens(query, super::schema::query_stopwords());
        if query_tokens.is_empty() {
            return Vec::new();
        }

        let mut matches = Vec::new();
        for (indexed_area, ids) in &self.by_area {
            if indexed_area.trim().is_empty() {
                continue;
            }
            for term in area_match_terms(indexed_area) {
                if area_term_matches_query(&term, &query_tokens) {
                    matches.push(IndexedAreaResolution {
                        name: display_area_term(&term),
                        indexed_area: indexed_area.clone(),
                        candidate_count: ids.len(),
                    });
                    break;
                }
            }
        }
        matches
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

fn compare_area_resolution(
    left: &IndexedAreaResolution,
    right: &IndexedAreaResolution,
) -> std::cmp::Ordering {
    left.name
        .split_whitespace()
        .count()
        .cmp(&right.name.split_whitespace().count())
        .then_with(|| left.name.len().cmp(&right.name.len()))
        .then_with(|| left.candidate_count.cmp(&right.candidate_count))
        .then_with(|| right.indexed_area.cmp(&left.indexed_area))
}

fn area_match_terms(area: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let normalized = normalize_area_text(area);
    push_area_term(&mut terms, &normalized);
    for segment in area.split([',', '/', '|']) {
        push_area_term(&mut terms, &normalize_area_text(segment));
    }
    for token in analyzer::search_tokens(area, super::schema::query_stopwords()) {
        if is_distinctive_area_token(&token) {
            push_area_term(&mut terms, &token);
        }
    }
    terms
}

fn normalize_area_text(value: &str) -> String {
    analyzer::search_tokens(value, super::schema::query_stopwords()).join(" ")
}

fn push_area_term(terms: &mut Vec<String>, term: &str) {
    let term = term.trim();
    if term.is_empty() || terms.iter().any(|existing| existing == term) {
        return;
    }
    if term.split_whitespace().count() == 1 && !is_distinctive_area_token(term) {
        return;
    }
    terms.push(term.to_string());
}

fn is_distinctive_area_token(token: &str) -> bool {
    token.len() >= 4
        && !matches!(
            token,
            "area"
                | "road"
                | "main"
                | "phase"
                | "stage"
                | "sector"
                | "layout"
                | "nagar"
                | "city"
                | "tower"
                | "west"
                | "east"
                | "north"
                | "south"
        )
}

fn area_term_matches_query(term: &str, query_tokens: &[String]) -> bool {
    let term_tokens = analyzer::search_tokens(term, super::schema::query_stopwords());
    if term_tokens.is_empty() {
        return false;
    }
    if term_tokens.len() == 1 {
        return query_tokens
            .iter()
            .any(|query_token| token_matches_query(query_token, &term_tokens[0]));
    }
    query_tokens.windows(term_tokens.len()).any(|window| {
        window
            .iter()
            .zip(term_tokens.iter())
            .all(|(query_token, term_token)| token_matches_query(query_token, term_token))
    })
}

fn area_term_is_excluded(query: &str, term: &str) -> bool {
    let normalized_query = normalize_area_text_for_exclusion(query);
    let normalized_term = normalize_area_text_for_exclusion(term);
    if normalized_term.is_empty() {
        return false;
    }
    [
        format!("not {}", normalized_term),
        format!("not in {}", normalized_term),
        format!("avoid {}", normalized_term),
        format!("exclude {}", normalized_term),
        format!("excluding {}", normalized_term),
        format!("except {}", normalized_term),
        format!("outside {}", normalized_term),
    ]
    .iter()
    .any(|pattern| query_contains_phrase(&normalized_query, pattern))
}

fn normalize_area_text_for_exclusion(value: &str) -> String {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter_map(|token| {
            let token = token.trim().to_lowercase();
            if token.len() >= 2 { Some(token) } else { None }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn display_area_term(term: &str) -> String {
    term.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let mut out = first.to_uppercase().collect::<String>();
                    out.push_str(chars.as_str());
                    out
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn push_unique(ids: &mut Vec<String>, id: &str) {
    if !ids.iter().any(|existing| existing == id) {
        ids.push(id.to_string());
    }
}

fn merge_score(scores: &mut HashMap<String, f64>, property_id: &str, score: f64) {
    scores
        .entry(property_id.to_string())
        .and_modify(|existing| *existing = existing.max(score))
        .or_insert(score);
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
    fn resolves_area_terms_from_indexed_property_area_without_config_alias() {
        let mut property = test_property("whitefield-3bhk", "prestige-waterford");
        property.area = "Itpl, Whitefield".to_string();
        let index = SearchIndex::build(&[property]);

        let resolved = index
            .resolve_query_area("3bhk in whitefield under 2cr", &[])
            .expect("indexed area token should resolve from runtime corpus");

        assert_eq!(resolved.name, "Whitefield");
        assert_eq!(resolved.indexed_area, "itpl, whitefield");
    }

    #[test]
    fn resolves_excluded_area_terms_from_indexed_property_area() {
        let mut property = test_property("electronic-city-3bhk", "snn-greenbay");
        property.area = "Phase 2 Electronic City".to_string();
        let index = SearchIndex::build(&[property]);

        let excluded =
            index.resolve_excluded_query_areas("not phase 2 electronic city, 3bhk near metro");

        assert_eq!(excluded[0].name, "Phase Electronic City");
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
