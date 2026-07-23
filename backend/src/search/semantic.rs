use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
#[cfg(feature = "fastembed")]
use std::sync::Mutex;

use serde::Serialize;

use crate::models::Property;
use crate::serving::{ServingEmbeddingRecord, ServingEntityRecord};

use super::schema;

/// Local query/document embedding boundary for semantic recall.
///
/// The scorer must not treat these vectors as proof. They only expand the
/// candidate pool and provide a soft ranking signal. Real model-backed providers
/// such as FastEmbed can implement this trait without changing the search path.
pub trait SemanticEmbedder: Send + Sync {
    fn model_id(&self) -> &'static str;
    fn dimensions(&self) -> usize;
    fn embed(&self, text: &str) -> Vec<f32>;
    fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>> {
        texts.iter().map(|text| self.embed(text)).collect()
    }
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

#[cfg(feature = "fastembed")]
pub struct FastEmbedSemanticEmbedder {
    model: Mutex<fastembed::TextEmbedding>,
    dimensions: usize,
}

#[cfg(feature = "fastembed")]
impl FastEmbedSemanticEmbedder {
    pub fn try_new_all_minilm_l6_v2() -> Result<Self, String> {
        let ort_dylib = std::env::var("OPENESTATES_ONNXRUNTIME_DYLIB").map_err(|_| {
            "OPENESTATES_ONNXRUNTIME_DYLIB must point at a local libonnxruntime.so when using fastembed"
                .to_string()
        })?;
        ort::init_from(&ort_dylib)
            .map_err(|err| format!("failed to load ONNX Runtime from {ort_dylib}: {err}"))?
            .commit();

        let model_name = fastembed::EmbeddingModel::AllMiniLML6V2;
        let dimensions = fastembed::TextEmbedding::get_model_info(&model_name)
            .map_err(|err| err.to_string())?
            .dim;
        let mut options =
            fastembed::TextInitOptions::new(model_name).with_show_download_progress(false);
        if let Some(intra_threads) = positive_env_usize("OPENESTATES_FASTEMBED_INTRA_THREADS") {
            options = options.with_intra_threads(intra_threads);
        } else {
            options = options.with_intra_threads(4);
        }
        let model = fastembed::TextEmbedding::try_new(options).map_err(|err| err.to_string())?;

        Ok(Self {
            model: Mutex::new(model),
            dimensions,
        })
    }
}

#[cfg(feature = "fastembed")]
impl SemanticEmbedder for FastEmbedSemanticEmbedder {
    fn model_id(&self) -> &'static str {
        "fastembed-all-minilm-l6-v2"
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        self.embed_batch(&[text.to_string()])
            .pop()
            .unwrap_or_else(|| vec![0.0; self.dimensions])
    }

    fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>> {
        let Ok(mut model) = self.model.lock() else {
            return vec![vec![0.0; self.dimensions]; texts.len()];
        };
        let batch_size = positive_env_usize("OPENESTATES_FASTEMBED_BATCH_SIZE").unwrap_or(64);
        let chunk_size = positive_env_usize("OPENESTATES_FASTEMBED_CHUNK_SIZE").unwrap_or(512);
        let mut embeddings = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(chunk_size) {
            let chunk_embeddings = match model.embed(chunk, Some(batch_size)) {
                Ok(embeddings) => embeddings,
                Err(err) => {
                    eprintln!("WARN: fastembed failed to embed text: {err}");
                    return vec![vec![0.0; self.dimensions]; texts.len()];
                }
            };
            for mut vector in chunk_embeddings {
                normalize(&mut vector);
                embeddings.push(vector);
            }
        }
        embeddings
    }
}

fn positive_env_usize(key: &str) -> Option<usize> {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticEmbeddingDocument {
    pub entity_id: String,
    pub entity_type: String,
    pub text: String,
}

impl SemanticSearchIndex {
    pub fn from_serving_entities(
        entities: &[ServingEntityRecord],
        embedder: &dyn SemanticEmbedder,
    ) -> Self {
        let documents = semantic_embedding_documents_from_serving_entities(entities);
        let document_keys = documents
            .iter()
            .map(|document| (document.entity_id.clone(), document.entity_type.clone()))
            .collect::<Vec<_>>();
        let texts = documents
            .iter()
            .map(|document| document.text.clone())
            .collect::<Vec<_>>();
        let vectors = embedder.embed_batch(&texts);
        let documents = document_keys
            .into_iter()
            .zip(vectors)
            .map(
                |((entity_id, entity_type), vector)| SemanticIndexedDocument {
                    entity_id,
                    entity_type,
                    vector,
                },
            )
            .collect();

        Self {
            model_id: embedder.model_id().to_string(),
            dimensions: embedder.dimensions(),
            documents,
        }
    }

    pub fn from_properties(properties: &[Property], embedder: &dyn SemanticEmbedder) -> Self {
        let document_keys = properties
            .iter()
            .map(|property| (format!("property:{}", property.id), "property".to_string()))
            .collect::<Vec<_>>();
        let texts = properties
            .iter()
            .map(|property| {
                bounded_semantic_text(format!(
                    "{} {} {} {} {} {} {} {}",
                    property.title,
                    property.area,
                    property.city,
                    property.society_id,
                    property.builder_name,
                    property.property_type,
                    property.possession_status,
                    property.description_summary
                ))
            })
            .collect::<Vec<_>>();
        let vectors = embedder.embed_batch(&texts);
        let documents = document_keys
            .into_iter()
            .zip(vectors)
            .map(
                |((entity_id, entity_type), vector)| SemanticIndexedDocument {
                    entity_id,
                    entity_type,
                    vector,
                },
            )
            .collect();

        Self {
            model_id: embedder.model_id().to_string(),
            dimensions: embedder.dimensions(),
            documents,
        }
    }

    pub fn from_embedding_records(
        records: &[ServingEmbeddingRecord],
        embedder: &dyn SemanticEmbedder,
    ) -> Self {
        let documents = records
            .iter()
            .filter(|record| {
                record.model_id == embedder.model_id()
                    && record.dimensions as usize == embedder.dimensions()
                    && matches!(record.entity_type.as_str(), "property" | "society")
                    && record.embedding.len() == embedder.dimensions()
            })
            .map(|record| SemanticIndexedDocument {
                entity_id: record.entity_id.clone(),
                entity_type: record.entity_type.clone(),
                vector: record.embedding.clone(),
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

pub fn semantic_embedding_documents_from_serving_entities(
    entities: &[ServingEntityRecord],
) -> Vec<SemanticEmbeddingDocument> {
    entities
        .iter()
        .filter(|entity| {
            matches!(entity.entity_type.as_str(), "property" | "society")
                && !entity.searchable_text.trim().is_empty()
        })
        .map(|entity| SemanticEmbeddingDocument {
            entity_id: entity.entity_id.clone(),
            entity_type: entity.entity_type.clone(),
            text: bounded_semantic_text(format!("{} {}", entity.name, entity.searchable_text)),
        })
        .collect()
}

fn semantic_tokens(text: &str) -> Vec<String> {
    let base_tokens = raw_tokens(text);
    let mut tokens = Vec::new();
    for token in &base_tokens {
        push_token(&mut tokens, token);
        for expanded in schema::semantic_expansion_tokens(token) {
            push_token(&mut tokens, expanded);
        }
    }
    for window in base_tokens.windows(2) {
        let phrase = format!("{} {}", window[0], window[1]);
        for expanded in schema::semantic_expansion_tokens(&phrase) {
            push_token(&mut tokens, expanded);
        }
        push_token(&mut tokens, &phrase.replace(' ', "_"));
    }
    tokens
}

fn bounded_semantic_text(text: String) -> String {
    let max_chars = positive_env_usize("OPENESTATES_SEMANTIC_DOCUMENT_MAX_CHARS").unwrap_or(2048);
    if text.len() <= max_chars {
        return text;
    }
    text.chars().take(max_chars).collect()
}

fn raw_tokens(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter_map(|token| {
            let token = token.trim().to_lowercase();
            if token.len() >= 2 && !schema::semantic_stopwords().contains(&token) {
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
