//! Community evidence summarization.
//!
//! The durable contract is source-neutral: Google reviews, Reddit threads, and
//! future community sources all become evidence records before they become KG
//! facts. Local ML can improve theme ranking or prose later, but structured
//! evidence remains the source of truth.

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub struct CommunityEvidenceRecord {
    pub entity_id: String,
    pub source_type: String,
    pub source_url: Option<String>,
    pub fact_key: String,
    pub text: Option<String>,
    pub numeric_value: Option<f64>,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub learned_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommunityEntitySummary {
    pub entity_id: String,
    pub summary: String,
    pub sentiment_score: Option<f64>,
    pub positive_themes: Vec<String>,
    pub concern_themes: Vec<String>,
    pub review_highlights: Vec<String>,
    pub source_urls: Vec<String>,
    pub evidence_count: usize,
    pub confidence: f32,
    pub learned_at: DateTime<Utc>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunityThemePolarity {
    Positive,
    Concern,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityThemeHit {
    pub key: String,
    pub label: String,
    pub polarity: CommunityThemePolarity,
}

pub trait CommunityThemeRanker {
    fn rank_themes(&self, documents: &[CommunityEvidenceRecord]) -> Vec<CommunityThemeHit>;
}

pub trait CommunitySummaryWriter {
    fn write_summary(&self, input: &CommunitySummaryInput<'_>) -> Option<String>;
    fn model_name(&self) -> Option<&'static str> {
        None
    }
}

#[derive(Debug)]
pub struct CommunitySummaryInput<'a> {
    pub entity_id: &'a str,
    pub source_types: &'a BTreeSet<String>,
    pub rating: Option<f64>,
    pub review_count: Option<u64>,
    pub positive_themes: &'a [String],
    pub concern_themes: &'a [String],
    pub evidence_texts: &'a [String],
    pub text_evidence_count: usize,
}

pub trait CommunityEvidenceSummaryEngine {
    fn summarize(&self, records: &[CommunityEvidenceRecord]) -> Vec<CommunityEntitySummary>;
}

#[derive(Debug, Default)]
pub struct DeterministicCommunityThemeRanker;

impl CommunityThemeRanker for DeterministicCommunityThemeRanker {
    fn rank_themes(&self, documents: &[CommunityEvidenceRecord]) -> Vec<CommunityThemeHit> {
        let mut hits = BTreeMap::<String, CommunityThemeHit>::new();
        for document in documents {
            if !has_textual_evidence(document) {
                continue;
            }
            let haystack = document_haystack(document);
            if haystack.trim().is_empty() {
                continue;
            }
            for theme in community_theme_candidates() {
                if theme
                    .terms
                    .iter()
                    .any(|term| contains_theme_term(&haystack, term))
                {
                    hits.entry(theme.key.to_string())
                        .or_insert_with(|| CommunityThemeHit {
                            key: theme.key.to_string(),
                            label: theme.label.to_string(),
                            polarity: theme.polarity.clone(),
                        });
                }
            }
        }
        hits.into_values().collect()
    }
}

#[derive(Debug, Clone)]
pub struct LocalEmbeddingCommunityThemeRanker {
    dimensions: usize,
    minimum_similarity: f64,
}

impl Default for LocalEmbeddingCommunityThemeRanker {
    fn default() -> Self {
        Self {
            dimensions: 256,
            minimum_similarity: 0.18,
        }
    }
}

impl LocalEmbeddingCommunityThemeRanker {
    pub fn new(dimensions: usize, minimum_similarity: f64) -> Self {
        Self {
            dimensions: dimensions.max(32),
            minimum_similarity,
        }
    }
}

impl CommunityThemeRanker for LocalEmbeddingCommunityThemeRanker {
    fn rank_themes(&self, documents: &[CommunityEvidenceRecord]) -> Vec<CommunityThemeHit> {
        let mut hits = BTreeMap::<String, CommunityThemeHit>::new();
        let theme_vectors = community_theme_candidates()
            .iter()
            .map(|theme| {
                let query_text = theme.evidence_queries.join(" ");
                (theme, local_text_embedding(&query_text, self.dimensions))
            })
            .collect::<Vec<_>>();

        for document in documents {
            if !has_textual_evidence(document) {
                continue;
            }
            let evidence_text = document_haystack(document);
            if evidence_text.trim().is_empty() {
                continue;
            }
            let evidence_vector = local_text_embedding(&evidence_text, self.dimensions);
            let evidence_tokens = local_embedding_tokens(&evidence_text)
                .into_iter()
                .collect::<BTreeSet<_>>();
            for (theme, theme_vector) in &theme_vectors {
                let exact_match = theme
                    .terms
                    .iter()
                    .any(|term| contains_theme_term(&evidence_text, term));
                let semantic_match = cosine_similarity(&evidence_vector, theme_vector)
                    >= self.minimum_similarity
                    && has_theme_anchor(&evidence_tokens, theme);
                if exact_match || semantic_match {
                    hits.entry(theme.key.to_string())
                        .or_insert_with(|| CommunityThemeHit {
                            key: theme.key.to_string(),
                            label: theme.label.to_string(),
                            polarity: theme.polarity.clone(),
                        });
                }
            }
        }
        hits.into_values().collect()
    }
}

fn contains_theme_term(haystack: &str, term: &str) -> bool {
    let normalized = term.to_ascii_lowercase();
    if normalized.contains(' ') {
        return haystack.contains(&normalized);
    }
    haystack
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|token| token == normalized)
}

fn has_theme_anchor(evidence_tokens: &BTreeSet<String>, theme: &CommunityThemeCandidate) -> bool {
    theme_anchor_tokens(theme)
        .iter()
        .any(|token| evidence_tokens.contains(token))
}

fn theme_anchor_tokens(theme: &CommunityThemeCandidate) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    tokens.extend(local_embedding_tokens(theme.key));
    tokens.extend(local_embedding_tokens(theme.label));
    for term in theme.terms {
        tokens.extend(local_embedding_tokens(term));
    }
    tokens
}

#[derive(Debug, Default)]
pub struct DeterministicCommunitySummaryWriter;

impl CommunitySummaryWriter for DeterministicCommunitySummaryWriter {
    fn write_summary(&self, input: &CommunitySummaryInput<'_>) -> Option<String> {
        Some(deterministic_summary(input))
    }
}

#[derive(Debug, Default)]
pub struct CommunityEvidenceSummarizer<R, W> {
    theme_ranker: R,
    summary_writer: W,
}

impl<R, W> CommunityEvidenceSummarizer<R, W>
where
    R: CommunityThemeRanker,
    W: CommunitySummaryWriter,
{
    pub fn new(theme_ranker: R, summary_writer: W) -> Self {
        Self {
            theme_ranker,
            summary_writer,
        }
    }

    pub fn summarize(&self, records: &[CommunityEvidenceRecord]) -> Vec<CommunityEntitySummary> {
        let mut by_entity = BTreeMap::<String, Vec<CommunityEvidenceRecord>>::new();
        for record in records {
            by_entity
                .entry(record.entity_id.clone())
                .or_default()
                .push(record.clone());
        }

        by_entity
            .into_iter()
            .filter_map(|(entity_id, records)| self.summarize_entity(entity_id, records))
            .collect()
    }

    fn summarize_entity(
        &self,
        entity_id: String,
        records: Vec<CommunityEvidenceRecord>,
    ) -> Option<CommunityEntitySummary> {
        if records.is_empty() {
            return None;
        }

        let source_types = records
            .iter()
            .map(|record| record.source_type.clone())
            .collect::<BTreeSet<_>>();
        let source_urls = source_urls(&records);
        let rating = latest_numeric(&records, "google_rating");
        let review_count =
            latest_numeric(&records, "google_review_count").map(|value| value as u64);
        let text_evidence_count = records
            .iter()
            .filter(|record| {
                record
                    .text
                    .as_deref()
                    .is_some_and(|text| text.split_whitespace().count() >= 3)
                    || !record.tags.is_empty()
            })
            .count();
        let evidence_texts = community_evidence_texts(&records);
        let theme_hits = self.theme_ranker.rank_themes(&records);
        let positive_themes = theme_labels(&theme_hits, CommunityThemePolarity::Positive);
        let concern_themes = theme_labels(&theme_hits, CommunityThemePolarity::Concern);
        let review_highlights = review_highlights(&evidence_texts);
        let input = CommunitySummaryInput {
            entity_id: &entity_id,
            source_types: &source_types,
            rating,
            review_count,
            positive_themes: &positive_themes,
            concern_themes: &concern_themes,
            evidence_texts: &evidence_texts,
            text_evidence_count,
        };
        let summary = self.summary_writer.write_summary(&input)?;
        let confidence = summary_confidence(&records, text_evidence_count);
        let learned_at = records
            .iter()
            .map(|record| record.learned_at)
            .max()
            .unwrap_or_else(Utc::now);

        Some(CommunityEntitySummary {
            entity_id,
            summary,
            sentiment_score: rating.map(|value| (value / 5.0 * 100.0).clamp(0.0, 100.0)),
            positive_themes,
            concern_themes,
            review_highlights,
            source_urls,
            evidence_count: records.len(),
            confidence,
            learned_at,
            model: self.summary_writer.model_name().map(str::to_string),
        })
    }
}

impl<R, W> CommunityEvidenceSummaryEngine for CommunityEvidenceSummarizer<R, W>
where
    R: CommunityThemeRanker,
    W: CommunitySummaryWriter,
{
    fn summarize(&self, records: &[CommunityEvidenceRecord]) -> Vec<CommunityEntitySummary> {
        CommunityEvidenceSummarizer::summarize(self, records)
    }
}

impl<T> CommunityThemeRanker for Box<T>
where
    T: CommunityThemeRanker + ?Sized,
{
    fn rank_themes(&self, documents: &[CommunityEvidenceRecord]) -> Vec<CommunityThemeHit> {
        (**self).rank_themes(documents)
    }
}

impl<T> CommunitySummaryWriter for Box<T>
where
    T: CommunitySummaryWriter + ?Sized,
{
    fn write_summary(&self, input: &CommunitySummaryInput<'_>) -> Option<String> {
        (**self).write_summary(input)
    }

    fn model_name(&self) -> Option<&'static str> {
        (**self).model_name()
    }
}

pub type DeterministicCommunityEvidenceSummarizer = CommunityEvidenceSummarizer<
    LocalEmbeddingCommunityThemeRanker,
    DeterministicCommunitySummaryWriter,
>;

pub fn deterministic_community_summarizer() -> DeterministicCommunityEvidenceSummarizer {
    CommunityEvidenceSummarizer::new(
        LocalEmbeddingCommunityThemeRanker::default(),
        DeterministicCommunitySummaryWriter,
    )
}

fn latest_numeric(records: &[CommunityEvidenceRecord], fact_key: &str) -> Option<f64> {
    records
        .iter()
        .filter(|record| record.fact_key == fact_key)
        .max_by_key(|record| record.learned_at)
        .and_then(|record| record.numeric_value)
}

fn source_urls(records: &[CommunityEvidenceRecord]) -> Vec<String> {
    let mut urls = records
        .iter()
        .filter_map(|record| record.source_url.clone())
        .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    urls.truncate(5);
    urls
}

fn community_evidence_texts(records: &[CommunityEvidenceRecord]) -> Vec<String> {
    let mut texts = records
        .iter()
        .flat_map(|record| {
            let mut values = Vec::new();
            if let Some(text) = record.text.as_deref() {
                values.push(text.to_string());
            }
            if record.fact_key == "google_review_snippets" {
                values.extend(record.tags.iter().cloned());
            }
            values
        })
        .map(|text| text.trim().to_string())
        .filter(|text| text.split_whitespace().count() >= 3)
        .collect::<Vec<_>>();
    texts.sort();
    texts.dedup();
    texts.truncate(12);
    texts
}

fn review_highlights(evidence_texts: &[String]) -> Vec<String> {
    let mut highlights = evidence_texts
        .iter()
        .filter(|text| !looks_like_generated_gap(text))
        .take(5)
        .cloned()
        .collect::<Vec<_>>();
    highlights.sort();
    highlights.dedup();
    highlights
}

fn looks_like_generated_gap(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("not ingested yet")
        || normalized.contains("not captured")
        || normalized.contains("not available")
}

fn theme_labels(hits: &[CommunityThemeHit], polarity: CommunityThemePolarity) -> Vec<String> {
    hits.iter()
        .filter(|hit| hit.polarity == polarity)
        .map(|hit| hit.label.clone())
        .collect()
}

fn summary_confidence(records: &[CommunityEvidenceRecord], text_evidence_count: usize) -> f32 {
    let average =
        records.iter().map(|record| record.confidence).sum::<f32>() / records.len().max(1) as f32;
    if text_evidence_count == 0 {
        average.min(0.55)
    } else {
        average.min(0.85)
    }
}

pub(crate) fn deterministic_summary(input: &CommunitySummaryInput<'_>) -> String {
    let source_label = if input.source_types.is_empty() {
        "Community"
    } else if input.source_types.len() == 1 && input.source_types.contains("Google") {
        "Google"
    } else if input.source_types.len() == 1 && input.source_types.contains("Reddit") {
        "Reddit"
    } else {
        "Community"
    };

    let mut parts = Vec::new();
    match (input.rating, input.review_count) {
        (Some(rating), Some(count)) => parts.push(format!(
            "{source_label} signal is {}: {:.1}/5 from {} reviews.",
            rating_band(rating),
            rating,
            count
        )),
        (Some(rating), None) => parts.push(format!(
            "{source_label} signal is {}: {:.1}/5.",
            rating_band(rating),
            rating
        )),
        (None, Some(count)) => parts.push(format!("{source_label} has {count} review signals.")),
        (None, None) => parts.push(format!("{source_label} evidence is present.")),
    }

    if !input.positive_themes.is_empty() {
        parts.push(format!(
            "Positive themes: {}.",
            input.positive_themes.join(", ")
        ));
    }
    if !input.concern_themes.is_empty() {
        parts.push(format!(
            "Watch themes: {}.",
            input.concern_themes.join(", ")
        ));
    }
    if input.text_evidence_count == 0 {
        parts.push(
            "Review text is not ingested yet, so theme-level claims are unavailable.".to_string(),
        );
    }

    parts.join(" ")
}

fn rating_band(rating: f64) -> &'static str {
    if rating >= 4.2 {
        "positive"
    } else if rating >= 3.8 {
        "mixed-positive"
    } else if rating >= 3.4 {
        "mixed"
    } else {
        "weak"
    }
}

fn document_haystack(document: &CommunityEvidenceRecord) -> String {
    let mut parts = Vec::new();
    parts.push(document.fact_key.as_str());
    if let Some(text) = document.text.as_deref() {
        parts.push(text);
    }
    for tag in &document.tags {
        parts.push(tag);
    }
    parts.join(" ").to_ascii_lowercase()
}

fn has_textual_evidence(document: &CommunityEvidenceRecord) -> bool {
    document
        .text
        .as_deref()
        .is_some_and(|text| text.split_whitespace().count() >= 3)
        || !document.tags.is_empty()
}

fn local_text_embedding(text: &str, dimensions: usize) -> Vec<f32> {
    let mut vector = vec![0.0_f32; dimensions];
    for token in local_embedding_tokens(text) {
        let index = hash_index(&token, dimensions);
        vector[index] += token_weight(&token);
    }
    normalize(&mut vector);
    vector
}

fn local_embedding_tokens(text: &str) -> Vec<String> {
    let raw = text
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter_map(|token| {
            let token = token.trim().to_ascii_lowercase();
            (token.len() >= 2 && !is_embedding_stopword(&token)).then_some(token)
        })
        .collect::<Vec<_>>();
    let mut tokens = Vec::new();
    for token in &raw {
        tokens.push(token.clone());
        tokens.extend(
            local_token_expansions(token)
                .iter()
                .map(|expanded| (*expanded).to_string()),
        );
    }
    for window in raw.windows(2) {
        let phrase = format!("{} {}", window[0], window[1]);
        tokens.push(phrase.replace(' ', "_"));
        tokens.extend(
            local_phrase_expansions(&phrase)
                .iter()
                .map(|expanded| (*expanded).to_string()),
        );
    }
    tokens
}

fn local_token_expansions(token: &str) -> &'static [&'static str] {
    match token {
        "green" | "greenery" | "trees" | "landscaped" | "garden" => {
            &["open_space", "calm", "nature"]
        }
        "maintained" | "maintenance" | "clean" | "upkeep" => &["maintenance", "well_kept"],
        "clubhouse" | "pool" | "gym" | "amenities" | "amenity" => {
            &["amenities", "clubhouse", "fitness"]
        }
        "metro" | "commute" | "connectivity" | "office" | "offices" => {
            &["connectivity", "transit", "tech_park"]
        }
        "traffic" | "congestion" | "jam" => &["traffic", "commute_risk"],
        "water" | "tanker" | "borewell" => &["water", "water_supply"],
        "noise" | "noisy" => &["noise", "quiet"],
        "parking" => &["parking", "visitor_parking"],
        _ => &[],
    }
}

fn local_phrase_expansions(phrase: &str) -> &'static [&'static str] {
    match phrase {
        "open space" => &["greenery", "calm", "large_campus"],
        "well maintained" => &["maintenance", "well_kept"],
        "tech park" | "tech parks" | "it park" | "it parks" => {
            &["connectivity", "office", "commute"]
        }
        "metro station" => &["metro", "connectivity", "commute"],
        "visitor parking" => &["parking"],
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

fn hash_index(token: &str, dimensions: usize) -> usize {
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

fn cosine_similarity(left: &[f32], right: &[f32]) -> f64 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }
    left.iter()
        .zip(right.iter())
        .map(|(left, right)| (*left as f64) * (*right as f64))
        .sum()
}

fn is_embedding_stopword(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "but"
            | "by"
            | "for"
            | "from"
            | "in"
            | "is"
            | "it"
            | "near"
            | "of"
            | "on"
            | "or"
            | "the"
            | "to"
            | "with"
    )
}

#[derive(Debug, Clone)]
pub struct CommunityThemeCandidate {
    pub key: &'static str,
    pub label: &'static str,
    pub polarity: CommunityThemePolarity,
    pub terms: &'static [&'static str],
    pub evidence_queries: &'static [&'static str],
}

pub fn community_theme_candidates() -> &'static [CommunityThemeCandidate] {
    &[
        CommunityThemeCandidate {
            key: "greenery",
            label: "greenery",
            polarity: CommunityThemePolarity::Positive,
            terms: &["green", "greenery", "trees", "tree", "open space", "park"],
            evidence_queries: &[
                "greenery trees landscaped open space calm layout",
                "parks and green open areas inside the society",
            ],
        },
        CommunityThemeCandidate {
            key: "maintenance",
            label: "maintenance",
            polarity: CommunityThemePolarity::Positive,
            terms: &["maintenance", "maintained", "clean", "well kept"],
            evidence_queries: &[
                "well maintained clean society upkeep",
                "good maintenance and responsive facility management",
            ],
        },
        CommunityThemeCandidate {
            key: "amenities",
            label: "amenities",
            polarity: CommunityThemePolarity::Positive,
            terms: &["clubhouse", "pool", "gym", "amenities", "play area"],
            evidence_queries: &[
                "clubhouse swimming pool gym play area amenities",
                "good society amenities for families",
            ],
        },
        CommunityThemeCandidate {
            key: "connectivity",
            label: "connectivity",
            polarity: CommunityThemePolarity::Positive,
            terms: &["metro", "connectivity", "commute", "tech park", "office"],
            evidence_queries: &[
                "metro commute connectivity tech parks offices",
                "easy access to work hubs and public transport",
            ],
        },
        CommunityThemeCandidate {
            key: "traffic",
            label: "traffic",
            polarity: CommunityThemePolarity::Concern,
            terms: &["traffic", "congestion", "jam", "commute issue"],
            evidence_queries: &[
                "traffic congestion jams commute delay",
                "road access problems and peak hour traffic",
            ],
        },
        CommunityThemeCandidate {
            key: "water",
            label: "water",
            polarity: CommunityThemePolarity::Concern,
            terms: &["water issue", "water problem", "tanker", "borewell"],
            evidence_queries: &[
                "water shortage tanker dependency borewell issue",
                "water supply problems in the society",
            ],
        },
        CommunityThemeCandidate {
            key: "noise",
            label: "noise",
            polarity: CommunityThemePolarity::Concern,
            terms: &["noise", "noisy", "construction noise"],
            evidence_queries: &[
                "noise problem construction noise noisy area",
                "sound disturbance around the society",
            ],
        },
        CommunityThemeCandidate {
            key: "parking",
            label: "parking",
            polarity: CommunityThemePolarity::Concern,
            terms: &["parking issue", "parking problem", "visitor parking"],
            evidence_queries: &[
                "parking problem visitor parking shortage",
                "car parking constraints and visitor parking issues",
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn google_rating_without_text_does_not_invent_themes() {
        let learned_at = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
        let summarizer = deterministic_community_summarizer();
        let summaries = summarizer.summarize(&[
            CommunityEvidenceRecord {
                entity_id: "society:example".to_string(),
                source_type: "Google".to_string(),
                source_url: Some("https://maps.google.com/example".to_string()),
                fact_key: "google_rating".to_string(),
                text: None,
                numeric_value: Some(3.9),
                tags: Vec::new(),
                confidence: 0.85,
                learned_at,
            },
            CommunityEvidenceRecord {
                entity_id: "society:example".to_string(),
                source_type: "Google".to_string(),
                source_url: Some("https://maps.google.com/example".to_string()),
                fact_key: "google_review_count".to_string(),
                text: None,
                numeric_value: Some(392.0),
                tags: Vec::new(),
                confidence: 0.85,
                learned_at,
            },
        ]);

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].sentiment_score, Some(78.0));
        assert!(summaries[0].positive_themes.is_empty());
        assert!(summaries[0].concern_themes.is_empty());
        assert!(summaries[0]
            .summary
            .contains("Review text is not ingested yet"));
        assert_eq!(summaries[0].confidence, 0.55);
        assert!(summaries[0].model.is_none());
    }

    #[test]
    fn text_evidence_produces_dynamic_themes() {
        let learned_at = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
        let summarizer = deterministic_community_summarizer();
        let summaries = summarizer.summarize(&[CommunityEvidenceRecord {
            entity_id: "society:example".to_string(),
            source_type: "Reddit".to_string(),
            source_url: Some("https://reddit.com/example".to_string()),
            fact_key: "resident_discussion".to_string(),
            text: Some(
                "Calm layout with many trees, clubhouse, pool, but traffic is bad.".to_string(),
            ),
            numeric_value: None,
            tags: Vec::new(),
            confidence: 0.7,
            learned_at,
        }]);

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].positive_themes, vec!["amenities", "greenery"]);
        assert_eq!(summaries[0].concern_themes, vec!["traffic"]);
        assert!(summaries[0].summary.contains("Positive themes"));
        assert!(!summaries[0]
            .summary
            .contains("Review text is not ingested yet"));
        assert_eq!(
            summaries[0].summary,
            "Reddit evidence is present. Positive themes: amenities, greenery. Watch themes: traffic."
        );
    }

    #[test]
    fn local_embeddings_do_not_create_unanchored_concern_themes() {
        let learned_at = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
        let summarizer = deterministic_community_summarizer();
        let summaries = summarizer.summarize(&[CommunityEvidenceRecord {
            entity_id: "society:prestige-lavender-fields".to_string(),
            source_type: "Google".to_string(),
            source_url: Some("https://maps.google.com/example".to_string()),
            fact_key: "google_review_snippets".to_string(),
            text: None,
            numeric_value: None,
            tags: vec![
                "Good place with modern amenities, a clubhouse and swimming pool.".to_string(),
                "Open space, central area, greenery and nearby tech park access.".to_string(),
                "Traffic near the approach road can be slow right now.".to_string(),
            ],
            confidence: 0.85,
            learned_at,
        }]);

        assert_eq!(summaries.len(), 1);
        assert_eq!(
            summaries[0].positive_themes,
            vec!["amenities", "connectivity", "greenery"]
        );
        assert_eq!(summaries[0].concern_themes, vec!["traffic"]);
        assert!(!summaries[0].summary.contains("noise"));
    }
}
