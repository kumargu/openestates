use std::collections::{HashMap, HashSet};

use crate::models::Property;
use crate::routes::enrichment::society_node_id;
use crate::serving::TantivyRecallHit;

use super::intent::{SearchIntent, AREA_ALIASES};
use super::semantic::SemanticRecallHit;

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
    by_token: HashMap<String, Vec<String>>,
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
        self.price_by_id.insert(property.id.clone(), property.price);

        let text = format!(
            "{} {} {} {} {} {} {}",
            property.title,
            property.area,
            property.city,
            property.society_id,
            property.builder_name,
            property.description_summary,
            property.transparency_tags.join(" ")
        );
        for token in tokenize(&text) {
            push_unique(self.by_token.entry(token).or_default(), &property.id);
        }
    }

    pub fn recall_ids(&self, query: &str, intent: &SearchIntent) -> Vec<String> {
        let mut candidate: Option<HashSet<String>> = None;

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
                    if *price <= budget_max {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .collect();
            intersect_candidate(&mut candidate, ids);
        }

        if candidate.is_none() {
            let token_ids = self.token_candidates(query);
            if !token_ids.is_empty() {
                candidate = Some(token_ids);
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
            if !hit.entity_id.starts_with("society:") {
                continue;
            }
            if let Some(property_ids) = self.by_society_node.get(&hit.entity_id) {
                for property_id in property_ids {
                    push_unique(&mut ids, property_id);
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

    fn area_candidates(&self, area: &str) -> HashSet<String> {
        let mut ids = HashSet::new();
        let area = normalize(area);
        self.extend_area_candidates(&area, &mut ids);

        for (aliases, canonical) in AREA_ALIASES {
            if !canonical.eq_ignore_ascii_case(&area) {
                continue;
            }
            for alias in *aliases {
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

    fn token_candidates(&self, query: &str) -> HashSet<String> {
        let mut ids = HashSet::new();
        for token in tokenize(query) {
            if is_query_stopword(&token) {
                continue;
            }
            if let Some(token_ids) = self.by_token.get(&token) {
                ids.extend(token_ids.iter().cloned());
            }
        }
        ids
    }
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

fn merge_score(scores: &mut HashMap<String, f64>, property_id: &str, score: f64) {
    scores
        .entry(property_id.to_string())
        .and_modify(|existing| *existing = existing.max(score))
        .or_insert(score);
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

fn tokenize(value: &str) -> Vec<String> {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter_map(|token| {
            let token = token.trim().to_lowercase();
            if token.len() >= 2 {
                Some(token)
            } else {
                None
            }
        })
        .collect()
}

fn is_query_stopword(token: &str) -> bool {
    matches!(
        token,
        "and"
            | "the"
            | "for"
            | "with"
            | "near"
            | "under"
            | "below"
            | "within"
            | "upto"
            | "bhk"
            | "cr"
            | "crore"
            | "lakhs"
            | "lakh"
            | "avoid"
            | "less"
            | "low"
            | "not"
            | "no"
    )
}
