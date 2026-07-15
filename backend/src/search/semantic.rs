use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::Serialize;

use crate::models::Property;
use crate::serving::ServingEntityRecord;

/// Local query/document embedding boundary for semantic recall.
///
/// The scorer must not treat these vectors as proof. They only expand the
/// candidate pool and provide a soft ranking signal. Real model-backed providers
/// such as FastEmbed can implement this trait without changing the search path.
pub trait SemanticEmbedder: Send + Sync {
    fn model_id(&self) -> &'static str;
    fn dimensions(&self) -> usize;
    fn embed(&self, text: &str) -> Vec<f32>;
}

/// Deterministic domain-aware fallback embedder.
///
/// This is not a neural model. It gives us a cheap local semantic contract in
/// default builds by expanding buyer-language synonyms before feature hashing.
/// Production neural embeddings should use the same `SemanticEmbedder` boundary.
#[derive(Debug, Clone)]
pub struct HashSemanticEmbedder {
    dimensions: usize,
}

impl Default for HashSemanticEmbedder {
    fn default() -> Self {
        Self { dimensions: 384 }
    }
}

impl HashSemanticEmbedder {
    pub fn new(dimensions: usize) -> Self {
        Self {
            dimensions: dimensions.max(16),
        }
    }
}

impl SemanticEmbedder for HashSemanticEmbedder {
    fn model_id(&self) -> &'static str {
        "openestates-domain-hash-v1"
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let mut vector = vec![0.0f32; self.dimensions];
        for token in semantic_tokens(text) {
            let index = hashed_index(&token, self.dimensions);
            vector[index] += token_weight(&token);
        }
        normalize(&mut vector);
        vector
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticRecallHit {
    pub entity_id: String,
    pub entity_type: String,
    pub score: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SemanticSearchIndex {
    model_id: String,
    dimensions: usize,
    documents: Vec<SemanticIndexedDocument>,
}

#[derive(Debug, Clone)]
struct SemanticIndexedDocument {
    entity_id: String,
    entity_type: String,
    vector: Vec<f32>,
}

impl SemanticSearchIndex {
    pub fn from_serving_entities(
        entities: &[ServingEntityRecord],
        embedder: &dyn SemanticEmbedder,
    ) -> Self {
        let documents = entities
            .iter()
            .filter(|entity| {
                matches!(entity.entity_type.as_str(), "property" | "society")
                    && !entity.searchable_text.trim().is_empty()
            })
            .map(|entity| {
                let text = format!("{} {}", entity.name, entity.searchable_text);
                SemanticIndexedDocument {
                    entity_id: entity.entity_id.clone(),
                    entity_type: entity.entity_type.clone(),
                    vector: embedder.embed(&text),
                }
            })
            .collect();

        Self {
            model_id: embedder.model_id().to_string(),
            dimensions: embedder.dimensions(),
            documents,
        }
    }

    pub fn from_properties(properties: &[Property], embedder: &dyn SemanticEmbedder) -> Self {
        let documents = properties
            .iter()
            .map(|property| {
                let text = format!(
                    "{} {} {} {} {} {} {} {}",
                    property.title,
                    property.area,
                    property.city,
                    property.society_id,
                    property.builder_name,
                    property.property_type,
                    property.possession_status,
                    property.description_summary
                );
                SemanticIndexedDocument {
                    entity_id: format!("property:{}", property.id),
                    entity_type: "property".to_string(),
                    vector: embedder.embed(&text),
                }
            })
            .collect();

        Self {
            model_id: embedder.model_id().to_string(),
            dimensions: embedder.dimensions(),
            documents,
        }
    }

    pub fn search(
        &self,
        query: &str,
        embedder: &dyn SemanticEmbedder,
        limit: usize,
    ) -> Vec<SemanticRecallHit> {
        if self.documents.is_empty()
            || limit == 0
            || self.model_id != embedder.model_id()
            || self.dimensions != embedder.dimensions()
        {
            return Vec::new();
        }

        let query_vector = embedder.embed(query);
        if query_vector.iter().all(|value| *value == 0.0) {
            return Vec::new();
        }

        let mut hits = self
            .documents
            .iter()
            .filter_map(|document| {
                let score = cosine_similarity(&query_vector, &document.vector);
                (score > 0.0).then(|| SemanticRecallHit {
                    entity_id: document.entity_id.clone(),
                    entity_type: document.entity_type.clone(),
                    score,
                })
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(limit);
        hits
    }

    pub fn len(&self) -> usize {
        self.documents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }
}

fn semantic_tokens(text: &str) -> Vec<String> {
    let base_tokens = raw_tokens(text);
    let mut tokens = Vec::new();
    for token in &base_tokens {
        push_token(&mut tokens, token);
        for expanded in token_expansions(token) {
            push_token(&mut tokens, expanded);
        }
    }
    for window in base_tokens.windows(2) {
        let phrase = format!("{} {}", window[0], window[1]);
        for expanded in phrase_expansions(&phrase) {
            push_token(&mut tokens, expanded);
        }
        push_token(&mut tokens, &phrase.replace(' ', "_"));
    }
    tokens
}

fn raw_tokens(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter_map(|token| {
            let token = token.trim().to_lowercase();
            if token.len() >= 2 && !is_stopword(&token) {
                Some(token)
            } else {
                None
            }
        })
        .collect()
}

fn push_token(tokens: &mut Vec<String>, token: &str) {
    if token.trim().is_empty() {
        return;
    }
    tokens.push(token.to_string());
}

fn token_expansions(token: &str) -> &'static [&'static str] {
    match token {
        "parents" | "senior" | "seniors" => &["family", "hospital", "quiet", "safe"],
        "kids" | "children" => &["family", "school", "play", "park"],
        "peaceful" | "calm" => &["quiet", "low_noise", "livability"],
        "cramped" => &["density", "open_space", "large_campus"],
        "spacious" => &["open_space", "large_campus"],
        "green" | "greenery" | "trees" | "landscaped" => &["open_space", "park", "nature"],
        "resident" | "residents" => &["community", "review", "lived_experience"],
        "marketing" | "hype" => &["builder_claim", "low_trust"],
        "enduse" | "enduser" => &["liveability", "occupancy", "maintenance"],
        "resale" => &["liquidity", "market", "investment"],
        "overpriced" => &["value", "price_per_sqft", "premium"],
        "traffic" | "congestion" => &["commute", "road", "risk"],
        "approach" => &["road", "access", "connectivity"],
        "hospital" => &["healthcare", "parents"],
        "school" | "schools" => &["family", "kids"],
        "office" | "offices" => &["tech_park", "commute"],
        "metro" => &["transit", "commute", "connectivity"],
        "clubhouse" | "pool" | "amenities" | "amenity" => &["amenity_quality", "society"],
        "safe" | "trusted" | "reliable" => &["risk", "builder", "verified"],
        _ => &[],
    }
}

fn phrase_expansions(phrase: &str) -> &'static [&'static str] {
    match phrase {
        "not cramped" => &["open_space", "large_campus", "low_density"],
        "actual residents" => &["resident", "community", "review", "lived_experience"],
        "investor hype" => &["marketing", "speculation", "low_trust"],
        "tech park" | "tech parks" | "it park" | "it parks" => {
            &["office", "commute", "employment_hub"]
        }
        "approach road" | "approach roads" => &["road", "access", "connectivity"],
        "good reviews" => &["review_quality", "google_rating", "resident"],
        "family friendly" => &["family", "school", "kids", "hospital"],
        _ => &[],
    }
}

fn token_weight(token: &str) -> f32 {
    if token.contains('_') {
        1.25
    } else {
        1.0
    }
}

fn hashed_index(token: &str, dimensions: usize) -> usize {
    let mut hasher = DefaultHasher::new();
    token.hash(&mut hasher);
    (hasher.finish() as usize) % dimensions
}

fn normalize(vector: &mut [f32]) {
    let norm = vector
        .iter()
        .map(|value| (*value as f64) * (*value as f64))
        .sum::<f64>()
        .sqrt();
    if norm == 0.0 {
        return;
    }
    for value in vector {
        *value = (*value as f64 / norm) as f32;
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    a.iter()
        .zip(b.iter())
        .map(|(left, right)| (*left as f64) * (*right as f64))
        .sum::<f64>()
}

fn is_stopword(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "but"
            | "for"
            | "from"
            | "i"
            | "if"
            | "in"
            | "is"
            | "it"
            | "near"
            | "not"
            | "of"
            | "or"
            | "the"
            | "to"
            | "want"
            | "with"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_hash_embedder_maps_parent_language_to_hospital_documents() {
        let embedder = HashSemanticEmbedder::new(128);
        let entities = vec![
            entity(
                "society:parents-fit",
                "society",
                "Parents Fit",
                "quiet community near Apollo hospital and pharmacy",
            ),
            entity(
                "society:party-fit",
                "society",
                "Party Fit",
                "nightlife bars restaurants and retail high street",
            ),
        ];
        let index = SemanticSearchIndex::from_serving_entities(&entities, &embedder);
        let hits = index.search("peaceful 3bhk for my parents", &embedder, 2);

        assert_eq!(
            hits.first().map(|hit| hit.entity_id.as_str()),
            Some("society:parents-fit")
        );
    }

    #[test]
    fn index_refuses_mismatched_embedding_model() {
        let embedder = HashSemanticEmbedder::new(128);
        let other_embedder = HashSemanticEmbedder::new(64);
        let entities = vec![entity(
            "society:one",
            "society",
            "One",
            "quiet family hospital",
        )];
        let index = SemanticSearchIndex::from_serving_entities(&entities, &embedder);

        assert!(index.search("parents", &other_embedder, 10).is_empty());
    }

    #[test]
    fn index_ignores_non_search_entity_types() {
        let embedder = HashSemanticEmbedder::new(128);
        let entities = vec![
            entity("builder:one", "builder", "Builder", "premium safe builder"),
            entity("society:one", "society", "Society", "premium safe society"),
        ];
        let index = SemanticSearchIndex::from_serving_entities(&entities, &embedder);

        assert_eq!(index.len(), 1);
    }

    fn entity(
        entity_id: &str,
        entity_type: &str,
        name: &str,
        searchable_text: &str,
    ) -> ServingEntityRecord {
        ServingEntityRecord {
            entity_id: entity_id.to_string(),
            entity_type: entity_type.to_string(),
            name: name.to_string(),
            root_source: None,
            searchable_text: searchable_text.to_string(),
        }
    }
}
