use std::collections::{HashMap, HashSet};

use crate::models::Property;
use crate::routes::enrichment::society_node_id;
use crate::serving::{
    unique_society_aliases, ServingEdgeRecord, ServingEntityRecord, TantivyRecallHit,
};

use super::analyzer;
use super::ast::{CompiledQuery, ConstraintExpr, ConstraintTerm};
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
    entity_name_by_id: HashMap<String, String>,
    root_source_by_entity: HashMap<String, String>,
    by_property_node: HashMap<String, String>,
    by_entity_node: HashMap<String, Vec<String>>,
    society_entity_by_property: HashMap<String, String>,
    builder_entity_by_property: HashMap<String, String>,
    area_entity_by_property: HashMap<String, String>,
    by_named_entity_phrase: HashMap<String, Vec<String>>,
    by_token: HashMap<String, Vec<String>>,
    position_by_id: HashMap<String, usize>,
    price_by_id: HashMap<String, u64>,
    price_min_by_id: HashMap<String, u64>,
    price_max_by_id: HashMap<String, u64>,
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
        Self::build_with_serving_graph(properties, entities, &[])
    }

    pub fn build_with_serving_graph(
        properties: &[Property],
        entities: &[ServingEntityRecord],
        edges: &[ServingEdgeRecord],
    ) -> Self {
        let mut index = Self::build(properties);
        for entity in entities {
            index
                .entity_name_by_id
                .insert(entity.entity_id.clone(), entity.name.clone());
            if let Some(root_source) = &entity.root_source {
                index
                    .root_source_by_entity
                    .insert(entity.entity_id.clone(), root_source.clone());
            }
        }
        index.add_serving_society_memberships(entities, edges);
        for (alias, canonical_id) in unique_society_aliases(entities) {
            let Some(property_ids) = index.by_entity_node.get(&alias).cloned() else {
                continue;
            };
            let canonical_property_ids = index
                .by_entity_node
                .entry(canonical_id.clone())
                .or_default();
            for property_id in &property_ids {
                push_unique(canonical_property_ids, &property_id);
            }
            for property_id in property_ids {
                index
                    .society_entity_by_property
                    .entry(property_id)
                    .or_insert_with(|| canonical_id.clone());
            }
        }
        index.add_serving_builder_memberships(entities, edges);
        index.add_serving_area_memberships(entities, edges);
        index
    }

    fn add_serving_society_memberships(
        &mut self,
        entities: &[ServingEntityRecord],
        edges: &[ServingEdgeRecord],
    ) {
        let society_ids = entities
            .iter()
            .filter(|entity| entity.entity_type.eq_ignore_ascii_case("society"))
            .map(|entity| entity.entity_id.as_str())
            .collect::<HashSet<_>>();
        for edge in edges
            .iter()
            .filter(|edge| edge.edge_type.eq_ignore_ascii_case("in_society"))
        {
            let Some(property_id) = self.by_property_node.get(&edge.from_entity_id).cloned() else {
                continue;
            };
            if !society_ids.contains(edge.to_entity_id.as_str()) {
                continue;
            }
            push_unique(
                self.by_entity_node
                    .entry(edge.to_entity_id.clone())
                    .or_default(),
                &property_id,
            );
            self.society_entity_by_property
                .entry(property_id)
                .or_insert_with(|| edge.to_entity_id.clone());
        }
    }

    fn add_serving_builder_memberships(
        &mut self,
        entities: &[ServingEntityRecord],
        edges: &[ServingEdgeRecord],
    ) {
        let entity_types = entities
            .iter()
            .map(|entity| (entity.entity_id.as_str(), entity.entity_type.as_str()))
            .collect::<HashMap<_, _>>();
        let mut additions = HashMap::<String, Vec<String>>::new();
        for edge in edges
            .iter()
            .filter(|edge| edge.edge_type.eq_ignore_ascii_case("built_by"))
        {
            let (society_id, builder_id) = match (
                entity_types.get(edge.from_entity_id.as_str()),
                entity_types.get(edge.to_entity_id.as_str()),
            ) {
                (Some(from), Some(to))
                    if from.eq_ignore_ascii_case("society")
                        && to.eq_ignore_ascii_case("builder") =>
                {
                    (edge.from_entity_id.as_str(), edge.to_entity_id.as_str())
                }
                (Some(from), Some(to))
                    if from.eq_ignore_ascii_case("builder")
                        && to.eq_ignore_ascii_case("society") =>
                {
                    (edge.to_entity_id.as_str(), edge.from_entity_id.as_str())
                }
                _ => continue,
            };
            let Some(property_ids) = self.by_entity_node.get(society_id).cloned() else {
                continue;
            };
            additions
                .entry(builder_id.to_string())
                .or_default()
                .extend(property_ids.iter().cloned());
            for property_id in property_ids {
                self.builder_entity_by_property
                    .entry(property_id)
                    .or_insert_with(|| builder_id.to_string());
            }
        }
        for (builder_id, property_ids) in additions {
            extend_unique(
                self.by_entity_node.entry(builder_id).or_default(),
                property_ids,
            );
        }
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
        for area_slug in [
            super::resolver::slug(&property.area),
            property.area_id.trim().to_string(),
        ] {
            if area_slug.is_empty() {
                continue;
            }
            push_unique(
                self.by_entity_node
                    .entry(format!("area:{area_slug}"))
                    .or_default(),
                &property.id,
            );
        }
        push_unique(self.by_bhk.entry(property.bhk).or_default(), &property.id);
        self.by_property_node
            .insert(format!("property:{}", property.id), property.id.clone());
        push_unique(
            self.by_entity_node
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
        let (price_min, price_max) =
            listing_price_bounds(property.price, property.price_min, property.price_max);
        self.price_min_by_id.insert(property.id.clone(), price_min);
        self.price_max_by_id.insert(property.id.clone(), price_max);

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

    pub fn recall_ids(&self, query: &CompiledQuery) -> Vec<String> {
        let mut candidate: Option<HashSet<String>> = None;

        let named_entity_ids = self.named_entity_candidates(&query.raw, &query.constraints);
        if !named_entity_ids.is_empty() {
            intersect_candidate(&mut candidate, named_entity_ids);
        }

        if query.constraints.has_terms() {
            intersect_candidate_exact(
                &mut candidate,
                self.constraint_candidates(&query.constraints),
            );
        }

        if candidate.is_none() {
            let token_ids = self.token_candidates_ranked(&query.raw);
            if !token_ids.is_empty() {
                return token_ids;
            }
        }

        let candidate = candidate.unwrap_or_else(|| self.all_ids.iter().cloned().collect());
        let ordered = self
            .all_ids
            .iter()
            .filter(|id| candidate.contains(*id))
            .cloned()
            .collect();
        ordered
    }

    pub fn recall_constraint_ids(&self, query: &CompiledQuery) -> Vec<String> {
        if !query.constraints.has_terms() {
            return self.all_ids.clone();
        }
        let candidates = self.constraint_candidates(&query.constraints);
        self.all_ids
            .iter()
            .filter(|id| candidates.contains(*id))
            .cloned()
            .collect()
    }

    fn constraint_candidates(&self, expr: &ConstraintExpr) -> HashSet<String> {
        match expr {
            ConstraintExpr::And { clauses } => {
                let mut clauses = clauses.iter();
                let Some(first) = clauses.next() else {
                    return self.all_ids.iter().cloned().collect();
                };
                let mut ids = self.constraint_candidates(first);
                for clause in clauses {
                    let next = self.constraint_candidates(clause);
                    ids.retain(|id| next.contains(id));
                }
                ids
            }
            ConstraintExpr::AnyOf { clauses } => {
                clauses.iter().fold(HashSet::new(), |mut ids, clause| {
                    ids.extend(self.constraint_candidates(clause));
                    ids
                })
            }
            ConstraintExpr::Not { clause } => {
                let excluded = self.constraint_candidates(clause);
                self.all_ids
                    .iter()
                    .filter(|id| !excluded.contains(*id))
                    .cloned()
                    .collect()
            }
            ConstraintExpr::Term { term } => self.term_candidates(term),
        }
    }

    fn term_candidates(&self, term: &ConstraintTerm) -> HashSet<String> {
        match term {
            ConstraintTerm::Bhk { value, .. } => self
                .by_bhk
                .get(value)
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
            ConstraintTerm::Area {
                entity_id: Some(entity_id),
                ..
            } => self
                .by_entity_node
                .get(entity_id)
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
            ConstraintTerm::Area { value, .. } => self.area_candidates(value),
            ConstraintTerm::Society { entity_id, .. }
            | ConstraintTerm::Builder { entity_id, .. } => self
                .by_entity_node
                .get(entity_id)
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
            ConstraintTerm::Budget { min, max, .. } => self
                .price_by_id
                .iter()
                .filter_map(|(id, price)| {
                    listing_satisfies_budget(
                        *price,
                        self.price_min_by_id.get(id).copied(),
                        self.price_max_by_id.get(id).copied(),
                        min.as_ref().map(|bound| bound.value),
                        max.as_ref().map(|bound| bound.value),
                    )
                    .then(|| id.clone())
                })
                .collect(),
            ConstraintTerm::Evidence { .. } => self.all_ids.iter().cloned().collect(),
        }
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
        self.by_entity_node
            .get(entity_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn entity_has_property(&self, entity_id: &str, property_id: &str) -> bool {
        self.by_property_node
            .get(entity_id)
            .is_some_and(|id| id == property_id)
            || self
                .by_entity_node
                .get(entity_id)
                .is_some_and(|ids| ids.iter().any(|id| id == property_id))
    }

    pub(crate) fn builder_entity_id_for_property(&self, property_id: &str) -> Option<&str> {
        self.builder_entity_by_property
            .get(property_id)
            .map(String::as_str)
    }

    pub(crate) fn society_entity_id_for_property(&self, property_id: &str) -> Option<&str> {
        self.society_entity_by_property
            .get(property_id)
            .map(String::as_str)
    }

    pub(crate) fn entity_name(&self, entity_id: &str) -> Option<&str> {
        self.entity_name_by_id.get(entity_id).map(String::as_str)
    }

    pub(crate) fn entity_root_source(&self, entity_id: &str) -> Option<&str> {
        self.root_source_by_entity
            .get(entity_id)
            .map(String::as_str)
    }

    pub(crate) fn area_entity_id_for_property(&self, property_id: &str) -> Option<&str> {
        self.area_entity_by_property
            .get(property_id)
            .map(String::as_str)
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

    fn add_serving_area_memberships(
        &mut self,
        entities: &[ServingEntityRecord],
        edges: &[ServingEdgeRecord],
    ) {
        let area_names = entities
            .iter()
            .filter(|entity| entity.entity_type.eq_ignore_ascii_case("area"))
            .map(|entity| (entity.entity_id.as_str(), entity.name.as_str()))
            .collect::<HashMap<_, _>>();
        let mut area_name_additions = HashMap::<String, Vec<String>>::new();
        let mut area_entity_additions = HashMap::<String, Vec<String>>::new();

        for edge in edges
            .iter()
            .filter(|edge| edge.edge_type.eq_ignore_ascii_case("in_area"))
        {
            let (society_entity_id, area_entity_id, area_name) =
                if let Some(area_name) = area_names.get(edge.to_entity_id.as_str()) {
                    (
                        edge.from_entity_id.as_str(),
                        edge.to_entity_id.as_str(),
                        *area_name,
                    )
                } else if let Some(area_name) = area_names.get(edge.from_entity_id.as_str()) {
                    (
                        edge.to_entity_id.as_str(),
                        edge.from_entity_id.as_str(),
                        *area_name,
                    )
                } else {
                    continue;
                };
            let Some(property_ids) = self.by_entity_node.get(society_entity_id).cloned() else {
                continue;
            };
            area_name_additions
                .entry(normalize(area_name))
                .or_default()
                .extend(property_ids.iter().cloned());
            area_entity_additions
                .entry(area_entity_id.to_string())
                .or_default()
                .extend(property_ids.iter().cloned());
            for property_id in property_ids {
                self.area_entity_by_property
                    .entry(property_id)
                    .or_insert_with(|| area_entity_id.to_string());
            }
        }
        for (area_name, property_ids) in area_name_additions {
            extend_unique(self.by_area.entry(area_name).or_default(), property_ids);
        }
        for (area_entity_id, property_ids) in area_entity_additions {
            extend_unique(
                self.by_entity_node.entry(area_entity_id).or_default(),
                property_ids,
            );
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

    fn named_entity_candidates(
        &self,
        query: &str,
        constraints: &ConstraintExpr,
    ) -> HashSet<String> {
        let query = normalize(query);
        if query.is_empty() {
            return HashSet::new();
        }
        self.by_named_entity_phrase
            .iter()
            .filter(|(phrase, _)| {
                !ast_excludes_named_entity(constraints, phrase, false)
                    && phrase_has_multiple_tokens(phrase)
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

fn intersect_candidate_exact(candidate: &mut Option<HashSet<String>>, next: HashSet<String>) {
    match candidate {
        Some(existing) => existing.retain(|id| next.contains(id)),
        None => *candidate = Some(next),
    }
}

fn ast_excludes_named_entity(expr: &ConstraintExpr, phrase: &str, negated: bool) -> bool {
    match expr {
        ConstraintExpr::And { clauses } | ConstraintExpr::AnyOf { clauses } => clauses
            .iter()
            .any(|clause| ast_excludes_named_entity(clause, phrase, negated)),
        ConstraintExpr::Not { clause } => ast_excludes_named_entity(clause, phrase, !negated),
        ConstraintExpr::Term {
            term: ConstraintTerm::Society { display_name, .. },
        } => negated && entity_keys_match(phrase, display_name),
        ConstraintExpr::Term {
            term: ConstraintTerm::Builder { display_name, .. },
        } => negated && entity_keys_match(phrase, display_name),
        ConstraintExpr::Term { .. } => false,
    }
}

fn push_unique(ids: &mut Vec<String>, id: &str) {
    if !ids.iter().any(|existing| existing == id) {
        ids.push(id.to_string());
    }
}

fn extend_unique(ids: &mut Vec<String>, additions: impl IntoIterator<Item = String>) {
    let mut seen = ids.iter().cloned().collect::<HashSet<_>>();
    ids.extend(additions.into_iter().filter(|id| seen.insert(id.clone())));
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

pub(crate) fn property_matches_excluded_society(property: &Property, name: &str) -> bool {
    entity_keys_match(&property.society_id, name)
        || query_contains_phrase(
            &normalize_entity_key(&property.title),
            &normalize_entity_key(name),
        )
}

pub(crate) fn property_matches_excluded_builder(property: &Property, name: &str) -> bool {
    entity_keys_match(&property.builder_name, name)
}

fn entity_keys_match(left: &str, right: &str) -> bool {
    let left = normalize_entity_key(left);
    let right = normalize_entity_key(right);
    !left.is_empty() && left == right
}

fn normalize_entity_key(value: &str) -> String {
    normalize(value)
        .replace('-', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[allow(dead_code)]
pub(crate) fn price_satisfies_budget(
    price: u64,
    budget_min: Option<u64>,
    budget_max: Option<u64>,
) -> bool {
    listing_satisfies_budget(price, None, None, budget_min, budget_max)
}

pub(crate) fn listing_satisfies_budget(
    price: u64,
    price_min: Option<u64>,
    price_max: Option<u64>,
    budget_min: Option<u64>,
    budget_max: Option<u64>,
) -> bool {
    let (low, high) = listing_price_bounds(price, price_min, price_max);
    if low == 0 && high == 0 {
        return false;
    }
    if let Some(min) = budget_min {
        if high < min {
            return false;
        }
    }
    if let Some(max) = budget_max {
        if low > max {
            return false;
        }
    }
    true
}

fn listing_price_bounds(price: u64, price_min: Option<u64>, price_max: Option<u64>) -> (u64, u64) {
    let low = price_min.filter(|value| *value > 0).unwrap_or(price);
    let high = price_max.filter(|value| *value > 0).unwrap_or(price);
    if low == 0 && high == 0 {
        (0, 0)
    } else if low <= high {
        (low, high)
    } else {
        (high, low)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::intent::SearchIntent;

    #[test]
    fn listing_band_overlaps_budget_instead_of_midpoint() {
        assert!(listing_satisfies_budget(
            32_250_000,
            Some(30_000_000),
            Some(48_000_000),
            None,
            Some(33_000_000),
        ));
        assert!(listing_satisfies_budget(
            32_250_000,
            Some(30_000_000),
            Some(48_000_000),
            Some(40_000_000),
            None,
        ));
        assert!(!listing_satisfies_budget(
            32_250_000,
            Some(30_000_000),
            Some(48_000_000),
            None,
            Some(29_000_000),
        ));
        assert!(listing_satisfies_budget(
            32_250_000,
            None,
            None,
            None,
            Some(33_000_000),
        ));
        assert!(price_satisfies_budget(32_250_000, None, Some(33_000_000)));
        assert!(!price_satisfies_budget(32_250_000, Some(40_000_000), None));
    }

    #[test]
    fn token_matches_query_handles_waterford_typo() {
        assert!(token_matches_query("wateford", "waterford"));
        assert!(token_matches_query("waterford", "waterford"));
        assert!(!token_matches_query("xyz", "waterford"));
    }

    #[test]
    fn bulk_membership_extension_is_stable_and_unique() {
        let mut ids = vec!["first".to_string()];
        extend_unique(
            &mut ids,
            ["second", "first", "second", "third"]
                .into_iter()
                .map(str::to_string),
        );

        assert_eq!(ids, vec!["first", "second", "third"]);
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

        assert_eq!(
            index.recall_ids(&CompiledQuery::from_text("Godrej Splendour")),
            vec!["prop-1"]
        );
        assert_eq!(
            index.recall_ids(&CompiledQuery::from_text("gorej")),
            vec!["prop-1"]
        );
    }

    #[test]
    fn recall_ids_handles_single_deletion_in_builder_token() {
        let property = test_property("prop-1", "brigade-7-gardens");
        let index = SearchIndex::build(&[property]);
        assert_eq!(
            index.recall_ids(&CompiledQuery::from_text("brgade")),
            vec!["prop-1"]
        );
    }

    #[test]
    fn recall_ids_unions_bhk_and_area_alternatives() {
        let mut two = test_property("two", "soc-two");
        two.bhk = 2;
        two.area = "East Bengaluru".to_string();
        let mut three = test_property("three", "soc-three");
        three.bhk = 3;
        three.area = "East Bengaluru".to_string();
        let mut four = test_property("four", "soc-four");
        four.bhk = 4;
        let mut sarjapur_two = test_property("sarjapur-two", "soc-sarj");
        sarjapur_two.bhk = 2;
        sarjapur_two.area = "Sarjapur".to_string();
        let index = SearchIndex::build(&[two, three, four, sarjapur_two]);

        let mut ids = index.recall_ids(&CompiledQuery::from_text("2 or 3 BHK"));
        ids.sort();
        assert_eq!(ids, vec!["sarjapur-two", "three", "two"]);

        let mut ids = index.recall_ids(&CompiledQuery::from_text(
            "2 or 3 BHK in Whitefield or Sarjapur",
        ));
        ids.sort();
        assert_eq!(ids, vec!["sarjapur-two", "three", "two"]);

        let mut ids = index.recall_ids(&CompiledQuery::from_text(
            "2 or 3 BHK in Whitefield or Sarjapur, not 2 BHK",
        ));
        ids.sort();
        assert_eq!(ids, vec!["three"]);

        let raw = "2 or 3 BHK in East Bengaluru";
        let mut ids = index.recall_ids(&CompiledQuery::from_text(raw));
        ids.sort();
        assert_eq!(ids, vec!["three", "two"]);
    }

    #[test]
    fn recall_executes_grouped_boolean_ast_without_cross_product() {
        let property = |id: &str, area: &str, bhk: u32| {
            let mut property = test_property(id, id);
            property.area = area.to_string();
            property.bhk = bhk;
            property
        };
        let properties = vec![
            property("whitefield-3", "Whitefield", 3),
            property("whitefield-2", "Whitefield", 2),
            property("bellandur-2", "Bellandur", 2),
            property("bellandur-3", "Bellandur", 3),
        ];
        let index = SearchIndex::build(&properties);
        let branch = |area: &str, bhk: u32| {
            ConstraintExpr::and(vec![
                ConstraintExpr::term(ConstraintTerm::Area {
                    entity_id: None,
                    value: area.to_string(),
                    span: None,
                }),
                ConstraintExpr::term(ConstraintTerm::Bhk {
                    value: bhk,
                    span: None,
                }),
            ])
        };
        let query = CompiledQuery {
            raw: "grouped query".to_string(),
            constraints: ConstraintExpr::any_of(vec![
                branch("Whitefield", 3),
                branch("Bellandur", 2),
            ]),
            intent: empty_intent(),
        };

        assert_eq!(
            index.recall_ids(&query),
            vec!["whitefield-3", "bellandur-2"]
        );
    }

    #[test]
    fn natural_language_grouped_alternatives_recall_only_requested_pairs() {
        let property = |id: &str, area: &str, bhk: u32| {
            let mut property = test_property(id, id);
            property.area = area.to_string();
            property.bhk = bhk;
            property
        };
        let properties = vec![
            property("east-3", "East Bengaluru", 3),
            property("east-2", "East Bengaluru", 2),
            property("south-2", "South Bengaluru", 2),
            property("south-3", "South Bengaluru", 3),
        ];
        let index = SearchIndex::build(&properties);
        let query = "3BHK in East Bengaluru or 2BHK in South Bengaluru";
        let compiled = CompiledQuery::from_text(query);

        assert_eq!(index.recall_ids(&compiled), vec!["east-3", "south-2"]);
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
        let compiled = CompiledQuery::from_text_with_intent("Prestige Falcon City", intent);

        assert_eq!(index.recall_ids(&compiled), vec!["falcon-3bhk"]);
    }

    #[test]
    fn excluded_society_does_not_seed_named_entity_recall() {
        let mut waterford = test_property("waterford-3bhk", "prestige-waterford");
        waterford.title = "3 BHK in Prestige Waterford".to_string();
        waterford.price = 34_500_000;
        let mut other = test_property("splendour-3bhk", "godrej-splendour");
        other.title = "3 BHK in Godrej Splendour".to_string();
        other.price = 17_200_000;
        let index = SearchIndex::build(&[waterford, other]);
        let constraints = ConstraintExpr::and(vec![
            ConstraintExpr::term(ConstraintTerm::Bhk {
                value: 3,
                span: None,
            }),
            ConstraintExpr::term(ConstraintTerm::Budget {
                min: None,
                max: Some(crate::search::ast::NumericBound {
                    value: 40_000_000,
                    inclusive: true,
                    raw_text: "under 4Cr".to_string(),
                }),
                span: None,
            }),
            ConstraintExpr::not(ConstraintExpr::term(ConstraintTerm::Society {
                entity_id: "society:prestige-waterford".to_string(),
                display_name: "Prestige Waterford".to_string(),
                span: None,
            })),
        ]);
        let compiled = CompiledQuery {
            raw: "3BHK under 4Cr, avoid Prestige Waterford".to_string(),
            constraints,
            intent: empty_intent(),
        };

        assert_eq!(index.recall_ids(&compiled), vec!["splendour-3bhk"]);
    }

    #[test]
    fn positive_builder_constraint_recalls_matching_properties() {
        let mut prestige = test_property("prestige-home", "prestige-society");
        prestige.builder_name = "Prestige".to_string();
        let mut brigade = test_property("brigade-home", "brigade-society");
        brigade.builder_name = "Brigade".to_string();
        let entities = vec![
            ServingEntityRecord {
                entity_id: "society:prestige-society".to_string(),
                entity_type: "society".to_string(),
                name: "Prestige Society".to_string(),
                root_source: None,
                searchable_text: "Prestige Society".to_string(),
            },
            ServingEntityRecord {
                entity_id: "society:brigade-society".to_string(),
                entity_type: "society".to_string(),
                name: "Brigade Society".to_string(),
                root_source: None,
                searchable_text: "Brigade Society".to_string(),
            },
            ServingEntityRecord {
                entity_id: "builder:prestige".to_string(),
                entity_type: "builder".to_string(),
                name: "Prestige".to_string(),
                root_source: None,
                searchable_text: "Prestige".to_string(),
            },
            ServingEntityRecord {
                entity_id: "builder:brigade".to_string(),
                entity_type: "builder".to_string(),
                name: "Brigade".to_string(),
                root_source: None,
                searchable_text: "Brigade".to_string(),
            },
        ];
        let edges = vec![
            ServingEdgeRecord {
                from_entity_id: "society:prestige-society".to_string(),
                to_entity_id: "builder:prestige".to_string(),
                edge_type: "built_by".to_string(),
                confidence: 1.0,
                source_type: "test".to_string(),
            },
            ServingEdgeRecord {
                from_entity_id: "society:brigade-society".to_string(),
                to_entity_id: "builder:brigade".to_string(),
                edge_type: "built_by".to_string(),
                confidence: 1.0,
                source_type: "test".to_string(),
            },
        ];
        let index = SearchIndex::build_with_serving_graph(&[prestige, brigade], &entities, &edges);
        let compiled = CompiledQuery {
            raw: "Prestige".to_string(),
            constraints: ConstraintExpr::term(ConstraintTerm::Builder {
                entity_id: "builder:prestige".to_string(),
                display_name: "Prestige".to_string(),
                span: None,
            }),
            intent: empty_intent(),
        };

        assert_eq!(index.recall_ids(&compiled), vec!["prestige-home"]);
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
        let compiled = CompiledQuery::from_text_with_intent("rating and reviews", intent);

        assert_eq!(index.recall_ids(&compiled), ["alpha", "beta"]);
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

    #[test]
    fn serving_graph_society_membership_maps_noncanonical_runtime_slugs() {
        let property = test_property("edge-linked-home", "legacy-runtime-key");
        let canonical_id = "society:rera-canonical-project";
        let entities = vec![ServingEntityRecord {
            entity_id: canonical_id.to_string(),
            entity_type: "society".to_string(),
            name: "Canonical Project".to_string(),
            root_source: Some("rera".to_string()),
            searchable_text: "Canonical Project".to_string(),
        }];
        let edges = vec![ServingEdgeRecord {
            from_entity_id: "property:edge-linked-home".to_string(),
            edge_type: "in_society".to_string(),
            to_entity_id: canonical_id.to_string(),
            confidence: 1.0,
            source_type: "test".to_string(),
        }];
        let index = SearchIndex::build_with_serving_graph(&[property], &entities, &edges);
        let query = CompiledQuery {
            raw: "Canonical Project".to_string(),
            constraints: ConstraintExpr::term(ConstraintTerm::Society {
                entity_id: canonical_id.to_string(),
                display_name: "Canonical Project".to_string(),
                span: None,
            }),
            intent: empty_intent(),
        };

        assert_eq!(index.recall_ids(&query), vec!["edge-linked-home"]);
        assert!(index.entity_has_property(canonical_id, "edge-linked-home"));
    }

    #[test]
    fn serving_graph_area_membership_participates_in_structured_recall() {
        let mut property = test_property("graph-area-home", "century-central");
        property.area = "Unknown".to_string();
        let entities = vec![
            ServingEntityRecord {
                entity_id: "society:rera-af36618d49c94b92".to_string(),
                entity_type: "society".to_string(),
                name: "Century Central".to_string(),
                root_source: Some("rera".to_string()),
                searchable_text: "Century Central".to_string(),
            },
            ServingEntityRecord {
                entity_id: "area:whitefield".to_string(),
                entity_type: "area".to_string(),
                name: "Whitefield".to_string(),
                root_source: Some("serving".to_string()),
                searchable_text: "Whitefield".to_string(),
            },
        ];
        let edges = vec![ServingEdgeRecord {
            from_entity_id: "society:rera-af36618d49c94b92".to_string(),
            edge_type: "in_area".to_string(),
            to_entity_id: "area:whitefield".to_string(),
            confidence: 0.9,
            source_type: "test".to_string(),
        }];
        let index = SearchIndex::build_with_serving_graph(&[property], &entities, &edges);
        let query = CompiledQuery {
            raw: "homes in Whitefield".to_string(),
            constraints: ConstraintExpr::term(ConstraintTerm::Area {
                entity_id: Some("area:whitefield".to_string()),
                value: "Whitefield".to_string(),
                span: None,
            }),
            intent: empty_intent(),
        };

        assert_eq!(index.recall_ids(&query), vec!["graph-area-home"]);
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
            price_min: None,
            price_max: None,
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
            excluded_societies: Vec::new(),
            excluded_builders: Vec::new(),
            areas: Vec::new(),
            bhk: None,
            bhks: Vec::new(),
            exclude_bhks: Vec::new(),
            bhk_spans: Vec::new(),
            budget_min: None,
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
